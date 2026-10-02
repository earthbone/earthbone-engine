// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const ACTOR: u32 = 0x100;
const OTHER: u32 = 0x101;
const PARALYSIS: u32 = 0x200;
const UNRELATED: u32 = 0x201;
const CURE: u32 = 0x202;
const PARALYSIS_SPELL: u32 = 0x300;
const UNRELATED_SPELL: u32 = 0x301;
const CURE_SPELL: u32 = 0x302;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn mgef(form: u32, archetype: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, 1 << 1);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, if archetype == 21 { 24 } else { u32::MAX });
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(Tag::new(b"MGEF"), form).sub(Tag::new(b"DATA"), data)
}

fn spell(form: u32, effect: u32, duration: u32) -> Record {
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    let mut efit = 1.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(Tag::new(b"SPEL"), form)
        .sub(Tag::new(b"SPIT"), spit)
        .sub(Tag::new(b"EFID"), effect.to_le_bytes())
        .sub(Tag::new(b"EFIT"), efit)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for actor in [ACTOR, OTHER] {
        plugin.push_top(
            Record::new(Tag::new(b"NPC_"), actor + 0x1000).sub(Tag::new(b"ACBS"), vec![0; 24]),
        );
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), (actor + 0x1000).to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    for (effect, archetype) in [(PARALYSIS, 21), (UNRELATED, 1), (CURE, 27)] {
        plugin.push_top(mgef(effect, archetype));
    }
    for (spell_id, effect, duration) in [
        (PARALYSIS_SPELL, PARALYSIS, 30),
        (UNRELATED_SPELL, UNRELATED, 30),
        (CURE_SPELL, CURE, 0),
    ] {
        plugin.push_top(spell(spell_id, effect, duration));
    }
    let mut order = LoadOrder::new();
    order.add("Mag48.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(game: &mut Game, spell: u32, target: u32) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(target)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_48_stacked_paralysis_cure_preserves_other_target_and_unrelated_effects() {
    let mut game = game();
    cast(&mut game, PARALYSIS_SPELL, ACTOR);
    cast(&mut game, PARALYSIS_SPELL, ACTOR);
    cast(&mut game, PARALYSIS_SPELL, OTHER);
    cast(&mut game, UNRELATED_SPELL, ACTOR);

    cast(&mut game, CURE_SPELL, ACTOR);

    assert!(!game.world().actor_is_paralyzed(id(ACTOR)));
    assert!(game.world().actor_is_paralyzed(id(OTHER)));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "paralysis"), 0.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "paralysis"), 1.0);
    assert!(
        game.world()
            .active_effects()
            .any(|effect| effect.target == id(ACTOR) && effect.effect == id(UNRELATED))
    );
}

#[test]
fn mag_48_saved_paralysis_remains_curable_after_load() {
    let mut original = game();
    cast(&mut original, PARALYSIS_SPELL, ACTOR);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert!(restored.world().actor_is_paralyzed(id(ACTOR)));

    cast(&mut restored, CURE_SPELL, ACTOR);

    assert!(!restored.world().actor_is_paralyzed(id(ACTOR)));
    assert_eq!(
        restored.world_mut().actor_value(id(ACTOR), "paralysis"),
        0.0
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;

const SOURCE: u32 = 0x100;
const TARGET_A: u32 = 0x101;
const TARGET_B: u32 = 0x102;
const CASTER: u32 = 0x103;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const BASE: u32 = 0x500;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 8, u32::MAX);
    put(&mut effect_data, 64, 45);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    for actor in [SOURCE, TARGET_A, TARGET_B, CASTER] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT).sub(Tag(*b"DATA"), effect_data));
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(
                Tag(*b"EFIT"),
                [0.0f32.to_le_bytes(), 0u32.to_le_bytes(), 30u32.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("GrabActorBreaker.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [SOURCE, TARGET_A, TARGET_B, CASTER] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(game: &mut Game, target: u32, caster: u32) -> EffectId {
    let shared = game.shared_world();
    let released = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(caster)),
            Some(id(target)),
            &mut |_, _| game.vm.create_object_of("GrabActorEffect", "ActiveMagicEffect"),
        )
        .unwrap()[0];
    released
}

#[test]
fn mag_66_each_effect_controls_only_its_selected_actor_and_finishes_independently() {
    let mut game = fixture();
    let first = cast(&mut game, TARGET_A, CASTER);
    let second = cast(&mut game, TARGET_B, SOURCE);
    assert_eq!(game.world().grab_actor_effect_on(id(TARGET_A)).unwrap().id, first);
    assert_eq!(game.world().grab_actor_effect_on(id(TARGET_B)).unwrap().id, second);

    let shared = game.shared_world();
    assert!(shared.borrow_mut().dispel_effect(first, &mut |name, class| {
        game.vm.create_object_of(name, class)
    }));
    assert!(game.world().grab_actor_effect_on(id(TARGET_A)).is_none());
    assert_eq!(game.world().grab_actor_effect_on(id(TARGET_B)).unwrap().id, second);
}

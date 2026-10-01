// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-34 stacked-effect break behavior from xEdit MGEF archetype 11.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const EFFECT_A: u32 = 0x200;
const EFFECT_B: u32 = 0x201;
const EFFECT_OTHER: u32 = 0x202;
const SPELL_A: u32 = 0x300;
const ACTIVATOR: u32 = 0x700;
const ACTIVATION_TARGET: u32 = 0x701;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn effect(form: u32, archetype: u32, actor_value: u32) -> Record {
    let mut data = vec![0; 152];
    data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    data[64..68].copy_from_slice(&archetype.to_le_bytes());
    data[68..72].copy_from_slice(&actor_value.to_le_bytes());
    data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    Record::new(T(b"MGEF"), form).sub(T(b"DATA"), data)
}

fn spell(form: u32, effects: &[u32]) -> Record {
    let mut data = vec![0; 36];
    data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut record = Record::new(T(b"SPEL"), form).sub(T(b"SPIT"), data);
    for &effect in effects {
        let mut entry = 1.0_f32.to_le_bytes().to_vec();
        entry.extend_from_slice(&0_u32.to_le_bytes());
        entry.extend_from_slice(&60_u32.to_le_bytes());
        record = record
            .sub(T(b"EFID"), effect.to_le_bytes())
            .sub(T(b"EFIT"), entry);
    }
    record
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"ACTI"), ACTIVATOR));
    plugin.push_top(
        Record::new(T(b"REFR"), ACTIVATION_TARGET).sub(T(b"NAME"), ACTIVATOR.to_le_bytes()),
    );
    plugin.push_top(effect(EFFECT_A, 11, 0));
    plugin.push_top(effect(EFFECT_B, 11, 0));
    plugin.push_top(effect(EFFECT_OTHER, 0, 24));
    plugin.push_top(spell(SPELL_A, &[EFFECT_A, EFFECT_B, EFFECT_OTHER]));

    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    game
}

#[test]
fn mag_34_activation_ends_every_invisibility_application_and_preserves_other_effects() {
    let mut game = game();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL_A),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |_, _| soe_papyrus::ObjectId(1),
        )
        .unwrap();

    assert!(
        game.world().is_invisible(id(ACTOR)),
        "active effects: {:?}",
        game.world()
            .active_effects()
            .map(|effect| (effect.target, effect.effect))
            .collect::<Vec<_>>()
    );
    assert!(game.activate(id(ACTIVATION_TARGET), id(ACTOR)));
    assert!(
        !game.world().is_invisible(id(ACTOR)),
        "all archetype 11 effects must end on reference activation"
    );
    let world = game.world();
    let remaining: Vec<_> = world
        .active_effects()
        .filter(|effect| effect.target == id(ACTOR))
        .collect();
    assert_eq!(remaining.len(), 1, "unrelated active effect must remain");
    assert_eq!(remaining[0].effect, id(EFFECT_OTHER));
}

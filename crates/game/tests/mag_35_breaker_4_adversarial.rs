// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Expiring one timed Light application must preserve another target's light.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const TARGET_A: u32 = 0x101;
const TARGET_B: u32 = 0x102;
const EFFECT: u32 = 0x200;
const SPELL_SHORT: u32 = 0x300;
const SPELL_LONG: u32 = 0x301;
const LIGHT: u32 = 0x502;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [CASTER, TARGET_A, TARGET_B] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    let mut mgef = vec![0; 152];
    mgef[8..12].copy_from_slice(&LIGHT.to_le_bytes());
    mgef[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    mgef[64..68].copy_from_slice(&12_u32.to_le_bytes());
    mgef[80..84].copy_from_slice(&1_u32.to_le_bytes());
    mgef[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));
    for (spell, duration) in [(SPELL_SHORT, 1_u32), (SPELL_LONG, 5_u32)] {
        let mut spit = vec![0; 36];
        spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
        spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
        let mut efit = 0.0_f32.to_le_bytes().to_vec();
        efit.extend_from_slice(&0_u32.to_le_bytes());
        efit.extend_from_slice(&duration.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"SPEL"), spell)
                .sub(T(b"SPIT"), spit)
                .sub(T(b"EFID"), EFFECT.to_le_bytes())
                .sub(T(b"EFIT"), efit),
        );
    }
    let mut light_data = vec![0; 48];
    light_data[4..8].copy_from_slice(&700_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"LIGH"), LIGHT).sub(T(b"DATA"), light_data));
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn mag_35_expiry_removes_only_the_light_owned_by_that_effect() {
    let mut game = game();
    {
        let world = game.shared_world();
        let mut world = world.borrow_mut();
        world
            .cast_spell(
                id(SPELL_SHORT), id(CASTER), Some(id(CASTER)), Some(id(TARGET_A)),
                &mut |_, _| soe_papyrus::ObjectId(1),
            )
            .unwrap();
        world
            .cast_spell(
                id(SPELL_LONG), id(CASTER), Some(id(CASTER)), Some(id(TARGET_B)),
                &mut |_, _| soe_papyrus::ObjectId(1),
            )
            .unwrap();
    }
    let reference_a = game.world().active_effects().find(|effect| effect.target == id(TARGET_A)).unwrap().summoned.unwrap();
    let reference_b = game.world().active_effects().find(|effect| effect.target == id(TARGET_B)).unwrap().summoned.unwrap();

    game.tick(1.0);
    assert!(game.world().reference_ref(reference_a).unwrap().deleted);
    assert!(!game.world().reference_ref(reference_b).unwrap().deleted);
    assert_eq!(game.world().active_effects().count(), 1);
}

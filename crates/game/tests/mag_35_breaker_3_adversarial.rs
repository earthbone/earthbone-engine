// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! MAG-35 expiry timing must continue from the saved active-effect clock.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const TARGET: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const LIGHT: u32 = 0x502;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [CASTER, TARGET] {
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

    let mut spit = vec![0; 36];
    spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 0.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut light_data = vec![0; 48];
    light_data[4..8].copy_from_slice(&700_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"LIGH"), LIGHT).sub(T(b"DATA"), light_data));
    plugin
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin()).unwrap();
    Game::new(order)
}

#[test]
fn mag_35_light_expiry_resumes_from_saved_elapsed_time() {
    let mut original = game();
    original
        .shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |_, _| soe_papyrus::ObjectId(1),
        )
        .unwrap();
    let spawned = original
        .world()
        .active_effects()
        .next()
        .unwrap()
        .summoned
        .unwrap();

    original.tick(0.75);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();

    restored.tick(1.24);
    assert_eq!(restored.world().active_effects().count(), 1);
    assert!(!restored.world().reference_ref(spawned).unwrap().deleted);

    restored.tick(0.01);
    assert_eq!(restored.world().active_effects().count(), 0);
    assert!(restored.world().reference_ref(spawned).unwrap().deleted);
    restored.tick(1.0);
    assert!(restored.world().reference_ref(spawned).unwrap().deleted);
}

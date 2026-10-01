// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-35 must validate the winning associated light record before mutation.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::MagicError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const TARGET: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const LIGHT: u32 = 0x502;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_35_rejects_a_non_light_winning_override_without_partial_application() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [ACTOR, TARGET] {
        base.push_top(
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
    base.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));
    let mut spit = vec![0; 36];
    spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 0.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&5_u32.to_le_bytes());
    base.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    base.push_top(Record::new(T(b"LIGH"), LIGHT));

    let mut override_plugin = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    override_plugin.push_top(Record::new(T(b"ACTI"), LIGHT));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Override.esp", override_plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [ACTOR, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }

    let result = game.shared_world().borrow_mut().cast_spell(
        id(SPELL),
        id(ACTOR),
        Some(id(ACTOR)),
        Some(id(TARGET)),
        &mut |_, _| soe_papyrus::ObjectId(1),
    );
    assert!(matches!(
        result,
        Err(MagicError::Unsupported("light record"))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(
        game.world().reference_ref(id(TARGET)).unwrap().base,
        Some(id(0x500))
    );
}

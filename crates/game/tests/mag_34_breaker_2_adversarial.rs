// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Blocked reference activation still exercises MAG-34's interaction break.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const SPELL: u32 = 0x300;
const EFFECT: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"ACTI"), 0x700));
    plugin.push_top(Record::new(T(b"REFR"), 0x701).sub(T(b"NAME"), 0x700_u32.to_le_bytes()));

    let mut mgef = vec![0; 152];
    mgef[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    mgef[64..68].copy_from_slice(&11_u32.to_le_bytes());
    mgef[80..84].copy_from_slice(&1_u32.to_le_bytes());
    mgef[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));

    let mut spit = vec![0; 36];
    spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 1.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&30_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );

    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game
}

#[test]
fn mag_34_blocked_reference_activation_ends_invisibility() {
    let mut game = fixture();
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |_, _| soe_papyrus::ObjectId(1),
        )
        .unwrap();
    assert!(game.world().is_invisible(id(ACTOR)));
    game.world_mut()
        .reference(id(0x701))
        .unwrap()
        .activation_blocked = true;

    assert!(!game.activate(id(0x701), id(ACTOR)));
    assert!(!game.world().is_invisible(id(ACTOR)));
}

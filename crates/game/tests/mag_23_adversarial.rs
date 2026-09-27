// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

#[test]
fn mag_23_remote_absorption_returns_cost_to_recipient_once() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    for actor in [0x100, 0x101] {
        plugin.push_top(Record::new(Tag::new(b"ACHR"), actor)
            .sub(Tag::new(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]));
    }
    let mut effect = vec![0; 152];
    effect[0..4].copy_from_slice(&1_u32.to_le_bytes());
    effect[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect[68..72].copy_from_slice(&24_u32.to_le_bytes());
    effect[80..84].copy_from_slice(&1_u32.to_le_bytes());
    effect[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200).sub(Tag::new(b"DATA"), effect));
    let mut spell = vec![0; 36];
    spell[0..4].copy_from_slice(&15_u32.to_le_bytes());
    spell[8..12].copy_from_slice(&0_u32.to_le_bytes());
    spell[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 10.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&5_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"SPEL"), 0x300)
        .sub(Tag::new(b"SPIT"), spell).sub(Tag::new(b"EFID"), 0x200_u32.to_le_bytes()).sub(Tag::new(b"EFIT"), efit));
    let mut order = LoadOrder::new();
    order.add("Absorb.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [0x100, 0x101] {
        game.world_mut().set_actor_value(id(actor), "health", 100.0).unwrap();
        game.world_mut().set_actor_value(id(actor), "magicka", 100.0).unwrap();
        game.world_mut().damage_actor_value(id(actor), "magicka", 30.0).unwrap();
    }
    game.world_mut().set_actor_value(id(0x101), "absorbchance", 100.0).unwrap();
    let shared = game.shared_world();
    let applied = shared.borrow_mut().cast_spell(id(0x300), id(0x100), Some(id(0x100)), Some(id(0x101)),
        &mut |name, class| game.vm.create_object_of(name, class)).unwrap();
    assert!(applied.is_empty());
    assert_eq!(game.world_mut().actor_value(id(0x101), "magicka"), 85.0);
    assert_eq!(game.world_mut().actor_value(id(0x100), "magicka"), 70.0);
}

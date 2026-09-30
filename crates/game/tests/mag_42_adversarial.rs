// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x100).sub(Tag::new(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 0x101)
        .sub(Tag::new(b"NAME"), 0x100u32.to_le_bytes())
        .sub(Tag::new(b"DATA"), vec![0; 24]));
    let mut effect = vec![0; 152];
    effect[64..68].copy_from_slice(&21u32.to_le_bytes());
    effect[80..84].copy_from_slice(&1u32.to_le_bytes());
    effect[84..88].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200).sub(Tag::new(b"DATA"), effect));
    let mut spell = vec![0; 36];
    spell[16..20].copy_from_slice(&1u32.to_le_bytes());
    spell[20..24].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"SPEL"), 0x300)
        .sub(Tag::new(b"SPIT"), spell)
        .sub(Tag::new(b"EFID"), 0x200u32.to_le_bytes())
        .sub(Tag::new(b"EFIT"), [
            1.0f32.to_le_bytes().as_slice(),
            0u32.to_le_bytes().as_slice(),
            2u32.to_le_bytes().as_slice(),
        ].concat()));
    let mut order = LoadOrder::new();
    order.add("Paralysis.esp", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().set_actor_value(id(0x101), "health", 100.0).unwrap();
    game
}

fn cast(game: &mut Game) {
    game.shared_world().borrow_mut().cast_spell(
        id(0x300), id(0x101), Some(id(0x101)), Some(id(0x101)),
        &mut |name, class| game.vm.create_object_of(name, class),
    ).unwrap();
}

#[test]
fn mag_42_paralysis_expires_after_saved_remaining_duration() {
    let mut game = fixture();
    cast(&mut game);
    assert!(game.world().actor_is_paralyzed(id(0x101)));
    game.tick(1.0);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture();
    loaded.load_state(&saved).unwrap();
    assert!(loaded.world().actor_is_paralyzed(id(0x101)));
    loaded.tick(2.0);
    assert!(!loaded.world().actor_is_paralyzed(id(0x101)));
}

#[test]
fn mag_42_paralysis_does_not_suppress_an_unaffected_actor() {
    let mut game = fixture();
    cast(&mut game);
    assert!(!game.world().actor_is_paralyzed(id(0x14)));
}

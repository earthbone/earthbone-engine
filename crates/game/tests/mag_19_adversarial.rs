// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn tapered_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag::new(b"ACHR"), 0x100)
            .sub(Tag::new(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    let mut data = vec![0; 152];
    data[0..4].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    data[64..68].copy_from_slice(&0_u32.to_le_bytes());
    data[68..72].copy_from_slice(&24_u32.to_le_bytes());
    data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    data[28..32].copy_from_slice(&0.5_f32.to_le_bytes());
    data[52..56].copy_from_slice(&2.0_f32.to_le_bytes());
    data[56..60].copy_from_slice(&4.0_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200).sub(Tag::new(b"DATA"), data));
    let mut spell = vec![0; 36];
    spell[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 10.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&4_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag::new(b"SPEL"), 0x300)
            .sub(Tag::new(b"SPIT"), spell)
            .sub(Tag::new(b"EFID"), 0x200_u32.to_le_bytes())
            .sub(Tag::new(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Taper.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x100), "health", 100.0)
        .unwrap();
    game
}

fn cast(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0x300),
            id(0x100),
            Some(id(0x100)),
            Some(id(0x100)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_19_fractional_final_tick_and_save_load_stop_at_taper_boundary() {
    let mut game = tapered_game();
    cast(&mut game);
    game.tick(7.5);
    let before_save = game.world_mut().actor_value(id(0x100), "health");
    assert!((before_save - 53.346354).abs() < 0.002);
    let bytes = game.save_state().unwrap();
    let mut loaded = tapered_game();
    loaded.load_state(&bytes).unwrap();
    loaded.tick(0.5);
    assert_eq!(loaded.world().active_effects().count(), 0);
    let final_health = loaded.world_mut().actor_value(id(0x100), "health");
    assert!((final_health - 53.333333).abs() < 0.002);
    loaded.tick(1.0);
    assert!((loaded.world_mut().actor_value(id(0x100), "health") - final_health).abs() < 0.0001);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag::new(b"ACHR"), 0x100)
            .sub(Tag::new(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag::new(b"PERK"), 0x700));
    let mut effect = vec![0; 152];
    effect[0..4].copy_from_slice(&(1_u32 << 1).to_le_bytes());
    effect[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect[68..72].copy_from_slice(&24_u32.to_le_bytes());
    effect[80..84].copy_from_slice(&1_u32.to_le_bytes());
    effect[84..88].copy_from_slice(&3_u32.to_le_bytes());
    effect[136..140].copy_from_slice(&0x700_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200).sub(Tag::new(b"DATA"), effect));
    for (spell_id, duration) in [(0x300, 2_u32), (0x301, 5)] {
        let mut data = vec![0; 36];
        data[16..20].copy_from_slice(&1_u32.to_le_bytes());
        data[20..24].copy_from_slice(&3_u32.to_le_bytes());
        let mut efit = 1.0_f32.to_le_bytes().to_vec();
        efit.extend_from_slice(&0_u32.to_le_bytes());
        efit.extend_from_slice(&duration.to_le_bytes());
        plugin.push_top(
            Record::new(Tag::new(b"SPEL"), spell_id)
                .sub(Tag::new(b"SPIT"), data)
                .sub(Tag::new(b"EFID"), 0x200_u32.to_le_bytes())
                .sub(Tag::new(b"EFIT"), efit),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Perks.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(game: &mut Game, spell: u32) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(0x100),
            Some(id(0x100)),
            Some(id(0x100)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_21_one_expiring_grant_does_not_remove_another_saved_live_grant() {
    let mut game = fixture();
    cast(&mut game, 0x300);
    cast(&mut game, 0x301);
    let bytes = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&bytes).unwrap();
    assert!(restored.world_mut().has_perk(id(0x100), id(0x700)));
    restored.tick(2.0);
    assert_eq!(restored.world().active_effects().count(), 1);
    assert!(restored.world_mut().has_perk(id(0x100), id(0x700)));
    restored.tick(3.0);
    assert!(!restored.world_mut().has_perk(id(0x100), id(0x700)));
}

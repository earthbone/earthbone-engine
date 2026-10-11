// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{UnresolvedRecord, UnresolvedRecordId};
use soe_game::Game;

fn plugin(tag: [u8; 4], object: u32) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(tag), object));
    plugin
}

fn order(files: &[(&str, Plugin)]) -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, plugin) in files {
        order.add(name, plugin.clone()).unwrap();
    }
    order
}

#[test]
fn sav_58_mixed_links_keep_identity_order_and_unrelated_pose_after_two_reloads() {
    let base = plugin(*b"STAT", 0x800);
    let removed = plugin(*b"STAT", 0x801);
    let mut retained = plugin(*b"STAT", 0x802);
    retained.push_top(Record::new(Tag(*b"NPC_"), 0x500).sub(Tag(*b"ACBS"), vec![0; 24]));
    retained.push_top(
        Record::new(Tag(*b"ACHR"), 0x101)
            .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let original = Game::new(order(&[
        ("base.esm", base.clone()),
        ("removed.esp", removed),
        ("retained.esp", retained.clone()),
    ]));
    let orphan_id = UnresolvedRecordId {
        plugin: "REMOVED.ESP".into(),
        light: false,
        object: 0x812,
    };
    let retained_link = UnresolvedRecordId {
        plugin: "retained.esp".into(),
        light: false,
        object: 0x321,
    };
    let missing_link = UnresolvedRecordId {
        plugin: "another-missing.esl".into(),
        light: true,
        object: 0x0abc,
    };
    let orphan = UnresolvedRecord {
        id: orphan_id,
        payload: vec![0xff, 0, 0x80, 0x10, 0, 0x80],
        links: vec![retained_link.clone(), missing_link.clone(), retained_link],
    };
    assert!(original.world_mut().set_host_actor_pose(
        GlobalFormId {
            file: 2,
            object: 0x101,
        },
        [31.0, -14.0, 8.0],
        [0.2, 0.3, 0.4],
    ));
    original
        .world_mut()
        .retain_unresolved_record(orphan.clone())
        .unwrap();
    let first = original.save_state().unwrap();

    let mut without_plugins = Game::new(order(&[
        ("base.esm", base.clone()),
        ("retained.esp", retained.clone()),
    ]));
    without_plugins.load_state(&first).unwrap();
    let after_first = without_plugins.save_state().unwrap();
    let mut reloaded = Game::new(order(&[
        ("base.esm", base.clone()),
        ("retained.esp", retained.clone()),
    ]));
    reloaded.load_state(&after_first).unwrap();
    let after_second = reloaded.save_state().unwrap();
    let mut final_load = Game::new(order(&[("base.esm", base), ("retained.esp", retained)]));
    final_load.load_state(&after_second).unwrap();

    assert_eq!(
        final_load.world().unresolved_records().get(&orphan.id),
        Some(&orphan)
    );
    assert_eq!(
        final_load
            .world()
            .ref_placement(GlobalFormId {
                file: 1,
                object: 0x101,
            })
            .unwrap()
            .position,
        [31.0, -14.0, 8.0]
    );
    assert_eq!(
        final_load
            .world()
            .record(GlobalFormId {
                file: 1,
                object: 0x802,
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
}

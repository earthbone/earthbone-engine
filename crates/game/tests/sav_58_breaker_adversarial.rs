// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial native-save checks for SAV-58.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{UnresolvedRecord, UnresolvedRecordId};
use soe_game::Game;

fn plugin(tag: [u8; 4], object: u32) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(tag), object));
    plugin
}

fn order(names: &[(&str, Plugin)]) -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, plugin) in names {
        order.add(name, plugin.clone()).unwrap();
    }
    order
}

fn unresolved(plugin: &str) -> UnresolvedRecord {
    UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: plugin.into(),
            light: false,
            object: 0x812,
        },
        payload: vec![0x00, 0x80, 0xff, 0x01],
        links: vec![UnresolvedRecordId {
            plugin: "later.esl".into(),
            light: true,
            object: 0x321,
        }],
    }
}

#[test]
fn sav_58_removed_regular_plugin_before_retained_plugin_keeps_world_identity() {
    let first = plugin(*b"STAT", 0x800);
    let middle = plugin(*b"STAT", 0x801);
    let mut tail = plugin(*b"STAT", 0x802);
    tail.push_top(Record::new(Tag(*b"NPC_"), 0x500).sub(Tag(*b"ACBS"), vec![0; 24]));
    tail.push_top(
        Record::new(Tag(*b"ACHR"), 0x101)
            .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let source_order = order(&[
        ("base.esm", first.clone()),
        ("removed.esp", middle),
        ("tail.esp", tail.clone()),
    ]);
    let source = Game::new(source_order);
    let orphan = unresolved("removed.esp");
    assert!(source.world_mut().set_host_actor_pose(
        soe_esm::GlobalFormId {
            file: 2,
            object: 0x101,
        },
        [11.0, 22.0, 33.0],
        [0.1, 0.2, 0.3],
    ));
    source
        .world_mut()
        .retain_unresolved_record(orphan.clone())
        .unwrap();
    let bytes = source.save_state().unwrap();

    let mut current = Game::new(order(&[
        ("base.esm", first.clone()),
        ("tail.esp", tail.clone()),
    ]));
    current.load_state(&bytes).unwrap();
    assert_eq!(
        current.world().unresolved_records().get(&orphan.id),
        Some(&orphan)
    );
    assert_eq!(
        current
            .world()
            .ref_placement(soe_esm::GlobalFormId {
                file: 1,
                object: 0x101,
            })
            .unwrap()
            .position,
        [11.0, 22.0, 33.0]
    );
    let next = current.save_state().unwrap();
    let mut reloaded = Game::new(order(&[("base.esm", first), ("tail.esp", tail)]));
    reloaded.load_state(&next).unwrap();
    assert_eq!(
        reloaded.world().unresolved_records().get(&orphan.id),
        Some(&orphan)
    );
    assert_eq!(
        reloaded
            .world()
            .ref_placement(soe_esm::GlobalFormId {
                file: 1,
                object: 0x101,
            })
            .unwrap()
            .position,
        [11.0, 22.0, 33.0]
    );
    assert_eq!(
        reloaded
            .world()
            .record(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
    assert_eq!(
        reloaded
            .world()
            .record(soe_esm::GlobalFormId {
                file: 1,
                object: 0x802
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
}

#[test]
fn sav_58_removed_light_plugin_before_retained_plugin_keeps_world_identity() {
    let light = |object| {
        let mut plugin = plugin(*b"STAT", object);
        plugin.header.flags = soe_esm::plugin::flags::LIGHT_MASTER;
        plugin
    };
    let base = plugin(*b"STAT", 0x800);
    let removed = light(0x801);
    let tail = light(0x802);
    let source = Game::new(order(&[
        ("base.esm", base.clone()),
        ("removed.esl", removed),
        ("tail.esl", tail.clone()),
    ]));
    let mut orphan = unresolved("removed.esl");
    orphan.id.light = true;
    orphan.id.object = 0x321;
    source
        .world_mut()
        .retain_unresolved_record(orphan.clone())
        .unwrap();
    let bytes = source.save_state().unwrap();

    let mut current = Game::new(order(&[
        ("base.esm", base.clone()),
        ("tail.esl", tail.clone()),
    ]));
    current.load_state(&bytes).unwrap();
    assert_eq!(
        current.world().unresolved_records().get(&orphan.id),
        Some(&orphan)
    );
    let next = current.save_state().unwrap();
    let mut reloaded = Game::new(order(&[("base.esm", base), ("tail.esl", tail)]));
    reloaded.load_state(&next).unwrap();
    assert_eq!(
        reloaded.world().unresolved_records().get(&orphan.id),
        Some(&orphan)
    );
    assert_eq!(
        reloaded
            .world()
            .record(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
    assert_eq!(
        reloaded
            .world()
            .record(soe_esm::GlobalFormId {
                file: 1,
                object: 0x802
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
}

#[test]
fn sav_58_rejects_same_name_changed_content_without_mutating_world() {
    let saved_plugin = plugin(*b"STAT", 0x800);
    let changed_plugin = plugin(*b"STAT", 0x801);
    let suffix = plugin(*b"STAT", 0x802);
    let source = Game::new(order(&[
        ("base.esm", saved_plugin),
        ("removed.esp", suffix.clone()),
    ]));
    source
        .world_mut()
        .retain_unresolved_record(unresolved("removed.esp"))
        .unwrap();
    let bytes = source.save_state().unwrap();

    let mut current = Game::new(order(&[("base.esm", changed_plugin)]));
    let before = current.save_state().unwrap();
    assert!(current.load_state(&bytes).is_err());
    assert_eq!(current.save_state().unwrap(), before);
}

#[test]
fn sav_58_rejects_added_plugin_after_removal_without_mutating_world() {
    let base = plugin(*b"STAT", 0x800);
    let removed = plugin(*b"STAT", 0x801);
    let source = Game::new(order(&[
        ("base.esm", base.clone()),
        ("removed.esp", removed),
    ]));
    source
        .world_mut()
        .retain_unresolved_record(unresolved("removed.esp"))
        .unwrap();
    let bytes = source.save_state().unwrap();

    let added = plugin(*b"STAT", 0x802);
    let mut current = Game::new(order(&[("base.esm", base), ("added.esp", added)]));
    let before = current.save_state().unwrap();
    assert!(current.load_state(&bytes).is_err());
    assert_eq!(current.save_state().unwrap(), before);
}

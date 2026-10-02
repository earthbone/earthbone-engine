// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-58 opaque unresolved identity and payload resilience.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::save::{DependencyChange, DependencyKind};
use soe_game::world::{UnresolvedRecord, UnresolvedRecordId, World};

fn live_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Live.esp", plugin).unwrap();
    order
}

fn pre_removal_order() -> LoadOrder {
    let mut order = live_order();
    order
        .add("absentregular.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    order
        .add(
            "absentlight.esl",
            Plugin::new(FileHeader {
                flags: soe_esm::plugin::flags::LIGHT_MASTER,
                ..FileHeader::default()
            }),
        )
        .unwrap();
    order
}

#[test]
fn sav_58_missing_regular_and_light_records_remain_opaque_for_two_saves() {
    let regular = UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: "absentregular.esp".into(),
            light: false,
            object: 0x00ff_ffff,
        },
        payload: vec![0, 1, 0xfe, 0xff, 0, 0x80],
        links: vec![UnresolvedRecordId {
            plugin: "absentlight.esl".into(),
            light: true,
            object: 0x0fff,
        }],
    };
    let light = UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: "absentlight.esl".into(),
            light: true,
            object: 0x0fff,
        },
        payload: vec![0xff, 0, 0x10, 0x80],
        links: vec![regular.id.clone()],
    };
    let mut source = World::new(pre_removal_order());
    source.retain_unresolved_record(regular.clone()).unwrap();
    source.retain_unresolved_record(light.clone()).unwrap();
    let first = source.save_state();

    let mut after_removal = World::new(live_order());
    after_removal.load_state(&first).unwrap();
    assert_eq!(
        after_removal.unresolved_records().get(&regular.id),
        Some(&regular)
    );
    assert_eq!(
        after_removal.unresolved_records().get(&light.id),
        Some(&light)
    );
    let second = after_removal.save_state();

    let mut reloaded = World::new(live_order());
    reloaded.load_state(&second).unwrap();
    assert_eq!(
        reloaded.unresolved_records().get(&regular.id),
        Some(&regular)
    );
    assert_eq!(reloaded.unresolved_records().get(&light.id), Some(&light));
    assert_eq!(
        reloaded
            .record(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
    assert_eq!(reloaded.save_state(), second);
}

#[test]
fn sav_58_unavailable_identity_never_aliases_a_live_form_with_same_object() {
    let unresolved = UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: "removed.esp".into(),
            light: false,
            object: 0x800,
        },
        payload: vec![9, 8, 7, 6],
        links: vec![UnresolvedRecordId {
            plugin: "removed.esl".into(),
            light: true,
            object: 0x800,
        }],
    };
    let mut source = World::new(pre_removal_order());
    source.retain_unresolved_record(unresolved.clone()).unwrap();
    let bytes = source.save_state();
    let mut restored = World::new(live_order());
    restored.load_state(&bytes).unwrap();

    assert_eq!(
        restored.unresolved_records().get(&unresolved.id),
        Some(&unresolved)
    );
    assert_eq!(
        restored
            .record(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
}

#[test]
fn sav_58_native_game_load_reports_removed_plugins_without_mutation() {
    let unresolved = UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: "absentregular.esp".into(),
            light: false,
            object: 0x812,
        },
        payload: vec![0x22, 0x40, 0xaa],
        links: vec![UnresolvedRecordId {
            plugin: "absentlight.esl".into(),
            light: true,
            object: 0x810,
        }],
    };
    let original = Game::new(pre_removal_order());
    original
        .world_mut()
        .retain_unresolved_record(unresolved.clone())
        .unwrap();
    let bytes = original.save_state().unwrap();

    let mut restored = Game::new(live_order());
    let report = restored.preflight_dependencies(&bytes).unwrap();
    assert_eq!(report.findings.len(), 2);
    assert!(report.findings.iter().all(|finding| {
        finding.kind == DependencyKind::Plugin && finding.change == DependencyChange::Missing
    }));

    let before = restored.save_state().unwrap();
    let error = restored.load_state(&bytes).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("saved plugin absentlight.esl is missing")
    );
    assert_eq!(restored.save_state().unwrap(), before);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::plugin::flags;
use soe_esm::{tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};

#[test]
fn deleted_winning_override_remains_the_winner_and_is_not_resurrected() {
    let mut base = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    base.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "Stone"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "StonePatch"));
    let mut deletion = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into(), "Patch.esp".into()],
        ..Default::default()
    });
    deletion.push_top(Record::new(tag::STAT, 0x800).with_flags(flags::DELETED));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    order.add("Delete.esp", deletion).unwrap();
    let global = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    assert!(order.winner(global).unwrap().is_deleted());
    assert_eq!(order.overrides(global).len(), 3);
}

#[test]
fn repeated_overrides_follow_load_order_not_record_insertion_order() {
    let make = |name: &'static str, masters: Vec<&str>| {
        let mut plugin = Plugin::new(FileHeader {
            masters: masters.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        });
        plugin.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, name));
        plugin
    };
    let mut order = LoadOrder::new();
    order.add("Base.esm", make("Base", vec![])).unwrap();
    order
        .add("First.esp", make("First", vec!["Base.esm"]))
        .unwrap();
    order
        .add("Second.esp", make("Second", vec!["Base.esm", "First.esp"]))
        .unwrap();
    let global = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    assert_eq!(
        order.winner(global).unwrap().editor_id().as_deref(),
        Some("Second")
    );
}

#[test]
fn light_runtime_ids_mask_object_bits_and_keep_light_slots_distinct() {
    let mut order = LoadOrder::new();
    for name in ["One.esl", "Two.esl"] {
        let plugin = Plugin::new(FileHeader {
            flags: flags::LIGHT_MASTER,
            ..Default::default()
        });
        order.add(name, plugin).unwrap();
    }
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: 0,
            object: 0x1ABC
        }),
        0xFE00_0ABC
    );
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: 1,
            object: 0x1ABC
        }),
        0xFE00_1ABC
    );
}

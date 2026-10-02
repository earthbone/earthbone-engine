// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::flags;
use soe_esm::{tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, CREATED_FILE};

#[test]
fn tst_03_created_form_ids_and_empty_order_queries() {
    let mut order = LoadOrder::new();
    assert!(order.is_empty());
    assert_eq!(order.len(), 0);
    assert!(!order.contains(GlobalFormId {
        file: 0,
        object: 0x800
    }));
    assert_eq!(
        order.overrides(GlobalFormId {
            file: 0,
            object: 0x800
        }),
        &[]
    );

    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "Stone"));
    order.add("Master.esm", plugin).unwrap();

    let stone = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    assert_eq!(stone.to_string(), "0:000800");
    assert!(!order.is_empty());
    assert_eq!(order.len(), 1);
    assert!(order.contains(stone));
    assert_eq!(order.runtime_id(stone), 0x0000_0800);
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: CREATED_FILE,
            object: 0x123,
        }),
        0xFF00_0123
    );
    assert_eq!(
        order.winners().map(|(id, _)| id).collect::<Vec<_>>(),
        [stone]
    );
    assert_eq!(order.winners_of(tag::STAT).count(), 1);
    assert!(order.overridden().next().is_none());
}

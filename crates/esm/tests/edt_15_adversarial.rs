// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::refr::Refr;
use soe_esm::records::Transform;
use soe_esm::{Plugin, Tag};

#[test]
fn edt_15_drop_can_be_written_as_refr_data_and_parsed_at_ground_height() {
    let before = Transform {
        position: [12.0, -8.0, 240.0],
        rotation: [0.1, 0.2, 0.3],
    };
    let placed = before.drop_to_ground(-5.5).unwrap().placed();
    let record = Record::new(Tag(*b"REFR"), 0x800)
        .sub(Tag(*b"NAME"), 0x1234u32.to_le_bytes())
        .sub(Tag(*b"DATA"), placed.to_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let refr = Refr::from_record(reloaded.records_of(Tag(*b"REFR")).next().unwrap()).unwrap();

    assert_eq!(refr.transform, Some(placed));
    assert_eq!(refr.transform.unwrap().position, [12.0, -8.0, -5.5]);
    assert_eq!(refr.transform.unwrap().rotation, before.rotation);
}

#[test]
fn edt_15_successive_drops_keep_independent_undo_transforms() {
    let before = Transform {
        position: [1.0, 2.0, 300.0],
        rotation: [0.4, 0.5, 0.6],
    };
    let first = before.drop_to_ground(10.0).unwrap();
    let second = first.placed().drop_to_ground(20.0).unwrap();

    assert_eq!(second.placed().position, [1.0, 2.0, 20.0]);
    assert_eq!(second.undo(), first.placed());
    assert_eq!(first.undo(), before);
}

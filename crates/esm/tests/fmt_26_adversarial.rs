// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::tag::{self, Tag};

fn plugin_with_link(raw: u32) -> Plugin {
    let header = FileHeader {
        masters: vec!["A.esm".into(), "B.esm".into()],
        ..FileHeader::default()
    };
    let mut plugin = Plugin::new(header);
    plugin.push_top(Record::new(tag::REFR, 0x0000_0801).sub(b"NAME", raw.to_le_bytes()));
    plugin
}

fn relocated_link(raw: u32) -> u32 {
    let mut plugin = plugin_with_link(raw);
    plugin
        .set_masters(vec!["B.esm".into(), "A.esm".into(), "C.esm".into()])
        .unwrap();
    let bytes = plugin.to_bytes();
    Plugin::parse(&bytes).unwrap().records[0]
        .find(Tag(*b"NAME"))
        .map(|data| u32::from_le_bytes(data.try_into().unwrap()))
        .unwrap()
}

#[test]
fn null_form_link_stays_null_after_master_reordering() {
    assert_eq!(relocated_link(0), 0);
}

#[test]
fn light_plugin_form_link_keeps_its_special_marker() {
    assert_eq!(relocated_link(0xFE00_1234), 0xFE00_1234);
}

#[test]
fn out_of_range_master_link_tracks_the_plugin_self_index() {
    assert_eq!(relocated_link(0x0500_1234), 0x0300_1234);
}

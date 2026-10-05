// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Checks malformed ambient links and DIAL subtype precedence.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(plugin: Plugin) -> soe_game::World {
    let mut order = LoadOrder::new();
    order.add("AUD11.esm", plugin).unwrap();
    soe_game::World::new(order)
}

#[test]
fn aud_11_missing_cell_region_link_is_rejected() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [0])
            .sub(T(b"XCLR"), 0x2000u32.to_le_bytes()),
    );
    let world = world(plugin);

    assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.5).is_err());
}

#[test]
fn aud_11_missing_acoustic_space_link_is_rejected() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [1, 0])
            .sub(T(b"XCAS"), 0x2000u32.to_le_bytes()),
    );
    let world = world(plugin);

    assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.5).is_err());
}

#[test]
fn aud_11_dial_snam_signature_overrides_legacy_numeric_subtype() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"DIAL"), 0x1000)
            .sub(T(b"DATA"), [0, 7, 94, 0])
            .sub(T(b"SNAM"), *b"HELO"),
    );
    let world = world(plugin);

    assert_eq!(
        world.dialogue_topic(id(0x1000)).unwrap().subtype,
        Some(soe_esm::records::gameplay::TopicSubtype::Signature(*b"HELO"))
    );
}

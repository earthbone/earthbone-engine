// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ambience, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn aud_09_missing_cell_acoustic_space_is_rejected() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x800)
            .sub(T(b"DATA"), [1_u8])
            .sub(T(b"XCAS"), 0x801_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let world = World::new(order);

    assert!(ambience::descriptors_for_cell(&world, id(0x800), 1, 0.0).is_err());
}

#[test]
fn aud_09_unresolved_cell_region_is_not_silently_treated_as_no_ambience() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x800)
            .sub(T(b"DATA"), [1_u8])
            .sub(T(b"XCLR"), 0x801_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let world = World::new(order);

    assert!(ambience::descriptors_for_cell(&world, id(0x800), 1, 0.0).is_err());
}

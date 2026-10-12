// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{music, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn aud_05_palette_cycle_is_rejected_instead_of_selecting_a_stale_leaf() {
    let music_id = 0x800;
    let palette_id = 0x801;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"MUSC"), music_id)
            .sub(T(b"FNAM"), 0_u32.to_le_bytes())
            .sub(T(b"PNAM"), [1, 0, 0, 0])
            .sub(T(b"TNAM"), palette_id.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), palette_id)
            .sub(T(b"CNAM"), 0x23F6_78C3_u32.to_le_bytes())
            .sub(T(b"SNAM"), palette_id.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);

    assert!(music::select(&mut game, id(music_id), id(1), 0.0).is_err());
}

#[test]
fn aud_05_missing_must_link_is_reported_as_invalid_record_data() {
    let music_id = 0x800;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"MUSC"), music_id)
            .sub(T(b"FNAM"), 0_u32.to_le_bytes())
            .sub(T(b"PNAM"), [1, 0, 0, 0])
            .sub(T(b"TNAM"), 0x801_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);

    assert!(music::select(&mut game, id(music_id), id(1), 1.0).is_err());
}

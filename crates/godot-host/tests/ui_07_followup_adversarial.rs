// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::race_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[test]
fn ui_07_rejects_playable_flag_in_truncated_race_data() {
    let mut data = vec![0; 36];
    data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), data));
    let mut order = LoadOrder::new();
    order.add("TruncatedRace.esm", plugin).unwrap();

    assert!(race_menu::entries(&Game::new(order)).is_empty());
}

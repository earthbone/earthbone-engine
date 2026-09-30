// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::race_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn race(id: u32, playable: bool) -> Record {
    let mut data = vec![0; 64];
    data[32..36].copy_from_slice(&u32::from(playable).to_le_bytes());
    Record::new(T(b"RACE"), id)
        .zsub(T(b"FULL"), &format!("Race {id}"))
        .sub(T(b"DATA"), data)
}

#[test]
fn ui_07_race_menu_uses_the_winning_override_playable_flag() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(race(0x700, true));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(race(0x700, false));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();

    assert!(race_menu::entries(&Game::new(order)).is_empty());
}

#[test]
fn ui_07_malformed_race_data_is_not_offered() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 0x701).sub(T(b"DATA"), vec![0; 35]));
    plugin.push_top(race(0x702, true));
    let mut order = LoadOrder::new();
    order.add("Malformed.esm", plugin).unwrap();
    let entries = race_menu::entries(&Game::new(order));

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].race, GlobalFormId { file: 0, object: 0x702 });
}

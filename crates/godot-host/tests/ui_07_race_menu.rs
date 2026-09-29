// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::race_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn race(object: u32, playable: bool, deleted: bool) -> Record {
    let mut data = vec![0; 128];
    data[32..36].copy_from_slice(&u32::from(playable).to_le_bytes());
    let record = Record::new(T(b"RACE"), object)
        .zsub(T(b"FULL"), &format!("Race {object}"))
        .sub(T(b"DATA"), data);
    if deleted {
        record.with_flags(flags::DELETED)
    } else {
        record
    }
}

#[test]
fn ui_07_race_menu_lists_only_winning_playable_race_records_in_form_order() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x702, true, false));
    plugin.push_top(race(0x700, false, false));
    plugin.push_top(race(0x701, true, true));
    plugin.push_top(race(0x703, true, false));
    let mut order = LoadOrder::new();
    order.add("Ui07Races.esm", plugin).unwrap();
    let game = Game::new(order);

    assert_eq!(
        race_menu::entries(&game)
            .iter()
            .map(|entry| entry.race)
            .collect::<Vec<_>>(),
        [id(0x702), id(0x703)]
    );
}

#[test]
fn ui_07_vanilla_playable_races_are_available_from_installed_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed race menu check");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let game = Game::new(order);
    let entries = race_menu::entries(&game);
    assert!(
        entries.len() >= 10,
        "vanilla playable races were not enumerated"
    );
    assert!(entries
        .iter()
        .all(|entry| game.world().record(entry.race).is_some()));
}

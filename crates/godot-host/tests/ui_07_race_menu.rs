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

fn head_part(object: u32, flags: u8, part_type: u32, deleted: bool) -> Record {
    let record = Record::new(T(b"HDPT"), object)
        .zsub(T(b"FULL"), &format!("Part {object}"))
        .sub(T(b"DATA"), [flags])
        .sub(T(b"PNAM"), part_type.to_le_bytes());
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
fn ui_14_head_part_catalog_filters_playability_and_sex_in_form_order() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(head_part(0x720, 5, 3, false));
    plugin.push_top(head_part(0x721, 3, 3, false));
    plugin.push_top(head_part(0x722, 7, 2, false));
    plugin.push_top(head_part(0x723, 5, 3, true));
    plugin.push_top(
        Record::new(T(b"RACE"), 0x700)
            .sub(T(b"HNAM"), 0x720_u32.to_le_bytes())
            .sub(T(b"HNAM"), 0x721_u32.to_le_bytes())
            .sub(T(b"ENAM"), 0x722_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ui14Parts.esm", plugin).unwrap();
    let game = Game::new(order);

    let parts = race_menu::head_parts(&game, id(0x700), true);
    assert_eq!(
        parts.iter().map(|part| part.form).collect::<Vec<_>>(),
        [id(0x720), id(0x722)]
    );
    assert_eq!(parts[0].part_type, 3);
    assert_eq!(parts[0].name, "Part 1824");
    assert_eq!(
        race_menu::head_parts(&game, id(0x700), false)
            .iter()
            .map(|part| part.form)
            .collect::<Vec<_>>(),
        [id(0x721), id(0x722)]
    );
}

#[test]
fn ui_14_head_part_and_morph_changes_update_preview_summary() {
    let game = Game::new(LoadOrder::new());
    let part = race_menu::HeadPartEntry {
        part_type: 3,
        form: id(0x720),
        name: "Test Hair".into(),
    };
    let mut appearance = soe_game::appearance::PlayerAppearance::default();
    let before = race_menu::preview_summary(&game, &appearance, std::slice::from_ref(&part));
    appearance.head_parts.insert(3, id(0x720));
    appearance.morphs.insert("Nose".into(), 0.4);
    let after = race_menu::preview_summary(&game, &appearance, &[part]);
    assert_ne!(before, after);
    assert!(after.contains("3: Test Hair"));
    assert!(after.contains("Nose 0.40"));
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

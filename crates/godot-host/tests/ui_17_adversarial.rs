// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_game::Game;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::race_menu;

#[test]
fn ui_17_adversarial_cached_head_parts_cover_distinct_models_and_morph_paths() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed chargen asset survey");
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let game = Game::from_shared_order(source.shared_order());
    let races = race_menu::entries(&game);
    assert!(!races.is_empty());
    let world = game.world();
    let mut model_paths = std::collections::BTreeSet::new();
    let mut morph_paths = std::collections::BTreeSet::new();
    for race in races {
        for female in [false, true] {
            for part in race_menu::head_parts(&game, race.race, female) {
                let record = world.order().winner(part.form).unwrap();
                if let Some(path) = record.find(soe_esm::Tag(*b"MODL")) {
                    let value = soe_esm::cursor::zstring(path);
                    if !value.is_empty() {
                        model_paths.insert(value.to_ascii_lowercase());
                    }
                }
                for (kind, path) in record
                    .all(soe_esm::Tag(*b"NAM0"))
                    .zip(record.all(soe_esm::Tag(*b"NAM1")))
                {
                    if kind == [2, 0, 0, 0] {
                        let value = soe_esm::cursor::zstring(path);
                        if !value.is_empty() {
                            morph_paths.insert(value.to_ascii_lowercase());
                        }
                    }
                }
            }
        }
    }
    assert!(!model_paths.is_empty(), "no head models are offered");
    assert!(!morph_paths.is_empty(), "no chargen TRI paths are offered");
    assert!(morph_paths.iter().all(|path| path.ends_with(".tri")));
}

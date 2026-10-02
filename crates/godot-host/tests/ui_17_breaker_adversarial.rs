// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::Tag;
use soe_game::Game;
use soe_godot_host::{data_source::DataSource, race_menu};

#[test]
fn ui_17_playable_race_catalog_repeats_and_resolves_head_model_and_morph_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed chargen source survey");
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let game = Game::from_shared_order(source.shared_order());
    let entries = race_menu::entries(&game);
    let world = game.world();
    assert!(
        !entries.is_empty(),
        "no playable race menu entries resolved"
    );
    assert_eq!(race_menu::entries(&game), entries);

    let mut resolved_parts = 0;
    let mut models = 0;
    let mut morphs = 0;
    for race in entries {
        for female in [false, true] {
            let parts = race_menu::head_parts(&game, race.race, female);
            assert_eq!(race_menu::head_parts(&game, race.race, female), parts);
            for part in parts {
                let record = world
                    .order()
                    .winner(part.form)
                    .expect("race catalog contained a missing head part");
                resolved_parts += 1;
                models += usize::from(
                    record
                        .find(Tag(*b"MODL"))
                        .is_some_and(|path| !soe_esm::cursor::zstring(path).is_empty()),
                );
                morphs += record
                    .all(Tag(*b"NAM0"))
                    .zip(record.all(Tag(*b"NAM1")))
                    .filter(|(kind, name)| {
                        **kind == [2, 0, 0, 0] && !soe_esm::cursor::zstring(name).is_empty()
                    })
                    .count();
            }
        }
    }
    assert!(resolved_parts > 0, "no playable race head parts resolved");
    assert!(models > 0, "no playable head parts define model paths");
    assert!(
        morphs > 0,
        "no playable head parts define chargen morph paths"
    );
}

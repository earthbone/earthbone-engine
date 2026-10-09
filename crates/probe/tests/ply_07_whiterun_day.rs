// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PLY-07 engine side: 24 game hours in Whiterun on the installed data.

use std::{collections::BTreeSet, path::PathBuf};

use soe_esm::{LoadOrder, Tag};
use soe_game::Game;
use soe_probe::{capture_engine_day, whiterun_actors};

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

fn installed_game() -> Option<Game> {
    let data = PathBuf::from(std::env::var_os("SKYRIM_DATA")?);
    let cache = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .join("soe-plugin-cache")
        .join("probe-installed");
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let plugin = soe_esm::read_plugin_cached(data.join(name), &cache)
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        order.add(name, plugin).unwrap();
    }
    Some(Game::new(order))
}

#[test]
fn ply_07_engine_records_every_whiterun_npc_each_game_hour() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping the Whiterun day capture");
        return;
    };
    let actors = whiterun_actors(&mut game);
    let labels: BTreeSet<&str> = actors.iter().map(|actor| actor.label.as_str()).collect();
    assert!(
        labels.contains("EorlundGrayMane"),
        "Eorlund Gray-Mane is a unique named Whiterun NPC, got {labels:?}"
    );
    assert!(actors.len() >= 20, "{} actors", actors.len());
    let table = capture_engine_day(&mut game, &actors).unwrap();
    assert_eq!(table.rows.len(), actors.len() * 24);
    let hours: BTreeSet<u8> = table.rows.iter().map(|row| row.hour).collect();
    assert_eq!(hours.len(), 24);
    for actor in &actors {
        let runtime = game.world().order().runtime_id(actor.reference);
        let own: Vec<_> = table
            .rows
            .iter()
            .filter(|row| row.actor == runtime)
            .collect();
        assert_eq!(own.len(), 24, "{}", actor.label);
        assert!(own.iter().all(|row| row.name == actor.label));
    }
    let _ = Tag(*b"ACHR");
    let eorlund = game.world().order().runtime_id(
        actors
            .iter()
            .find(|actor| actor.label == "EorlundGrayMane")
            .unwrap()
            .reference,
    );
    let packages: BTreeSet<Option<u32>> = table
        .rows
        .iter()
        .filter(|row| row.actor == eorlund)
        .map(|row| row.package)
        .collect();
    eprintln!(
        "PLY-07 {} actors, Eorlund packages {packages:?}",
        actors.len()
    );
    for row in table.rows.iter().filter(|row| row.actor == eorlund) {
        eprintln!("PLY-07 {row:?}");
    }
    let moving = actors
        .iter()
        .filter(|actor| {
            let runtime = game.world().order().runtime_id(actor.reference);
            let distinct: BTreeSet<_> = table
                .rows
                .iter()
                .filter(|row| row.actor == runtime)
                .map(|row| (row.package, row.cell, row.furniture))
                .collect();
            distinct.len() > 1
        })
        .count();
    eprintln!(
        "PLY-07 {moving} of {} actors change state during the day",
        actors.len()
    );
}

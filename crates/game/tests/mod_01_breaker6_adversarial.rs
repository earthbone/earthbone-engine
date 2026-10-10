// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x806;
const SPELL: u32 = 0x801;
const ACTORS: [u32; 3] = [0x812, 0x813, 0x814];
const PLAYER: u64 = 0x2A73_F01A;
static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

struct DataDir(PathBuf);

impl DataDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mod01-breaker6-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("Seed_DISTR.ini"),
            "Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|50!\n",
        )
        .unwrap();
        Self(path)
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn game(data: &DataDir, player: u64) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"SPEL"), SPELL).zsub(T(b"EDID"), "SeedSpell"));
    plugin.push_top(npc::build_record(
        BASE,
        "SeedBase",
        "Seed Base",
        Config::default(),
        Stats::default(),
    ));
    for actor in ACTORS {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0u8; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    game.world_mut().set_player_character_id(player);
    game.apply_mod_data(std::slice::from_ref(&data.0));
    game
}

fn has_spell(game: &mut Game, actor: u32) -> bool {
    game.world_mut()
        .actor(id(actor))
        .unwrap()
        .spells
        .contains(&id(SPELL))
}

#[test]
fn mod_01_two_actors_get_distinct_upstream_seeded_results() {
    let data = DataDir::new();
    let mut game = game(&data, PLAYER);

    // This fixture's source-entry, player ID, and runtime actor IDs produce
    // these two results with SPID's hash_combine and CLibUtil RNG path.
    assert!(!has_spell(&mut game, ACTORS[0]));
    assert!(has_spell(&mut game, ACTORS[2]));
}

#[test]
fn mod_01_seeded_actor_result_repeats_across_fresh_game_instances() {
    let data = DataDir::new();
    let mut first = game(&data, PLAYER);
    let first_results: Vec<_> = ACTORS
        .iter()
        .map(|actor| has_spell(&mut first, *actor))
        .collect();

    let mut second = game(&data, PLAYER);
    let second_results: Vec<_> = ACTORS
        .iter()
        .rev()
        .map(|actor| has_spell(&mut second, *actor))
        .collect();

    assert_eq!(first_results, [false, false, true]);
    assert_eq!(second_results, [true, false, false]);
}

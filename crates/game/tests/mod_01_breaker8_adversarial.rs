// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use std::{fs, path::PathBuf};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x806;
const SPELL: u32 = 0x801;
const ACTORS: [u32; 3] = [0x812, 0x813, 0x814];
const PLAYER_ID: u64 = 0x2A73_F01A;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("mod01-breaker8-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("Seed_DISTR.ini"),
            "Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|50!\n",
        )
        .unwrap();
        Self(directory)
    }

    fn game(&self) -> Game {
        let mut plugin = Plugin::new(FileHeader {
            flags: flags::MASTER,
            ..Default::default()
        });
        plugin.push_top(Record::new(T(b"SPEL"), SPELL).zsub(T(b"EDID"), "SeedSpell"));
        plugin.push_top(npc::build_record(
            BASE,
            "SeedBase",
            "Seed Base",
            npc::Config::default(),
            npc::Stats::default(),
        ));
        for actor in ACTORS {
            plugin.push_top(
                Record::new(T(b"ACHR"), actor)
                    .sub(T(b"NAME"), BASE.to_le_bytes())
                    .sub(T(b"DATA"), vec![0; 24]),
            );
        }
        let mut order = LoadOrder::new();
        order
            .add("Skyrim.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
            .unwrap();
        let mut game = Game::new(order);
        game.world_mut().set_player_character_id(PLAYER_ID);
        game.apply_mod_data(std::slice::from_ref(&self.0));
        game
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mod_01_same_base_actors_keep_distinct_upstream_seeded_results() {
    let fixture = Fixture::new();
    let mut game = fixture.game();
    let observed: Vec<_> = ACTORS
        .iter()
        .map(|actor| {
            game.world_mut()
                .actor(id(*actor))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();

    // SPID's hash_combine inputs are player ID then actor runtime FormID.
    assert_eq!(observed, [false, false, true]);
}

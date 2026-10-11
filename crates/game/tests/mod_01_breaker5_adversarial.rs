// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};
use std::{fs, path::PathBuf};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x806;
const SPELL: u32 = 0x801;
const FIRST_ACTOR: u32 = 0x812;
const PLAYER_ID: u64 = 0x2A73_F01A;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

struct DataDir(PathBuf);

impl DataDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("mod01-breaker5-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
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

fn build_game() -> Game {
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
    for offset in 0..3 {
        plugin.push_top(
            Record::new(T(b"ACHR"), FIRST_ACTOR + offset)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0u8; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

#[test]
fn mod_01_actor_seed_is_independent_of_first_access_order() {
    let data = DataDir::new();
    let mut game = build_game();
    game.world_mut().set_player_character_id(PLAYER_ID);
    game.apply_mod_data(std::slice::from_ref(&data.0));

    // SPID's LookupFilters seed and CLibUtil draw produce these outcomes for
    // the first three runtime FormIDs in this fixture.
    let expected = [false, false, true];
    let reverse: Vec<_> = (0..3)
        .rev()
        .map(|offset| {
            game.world_mut()
                .actor(id(FIRST_ACTOR + offset))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    assert_eq!(reverse, expected.into_iter().rev().collect::<Vec<_>>());

    let forward: Vec<_> = (0..3)
        .map(|offset| {
            game.world_mut()
                .actor(id(FIRST_ACTOR + offset))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    assert_eq!(forward, expected);
}

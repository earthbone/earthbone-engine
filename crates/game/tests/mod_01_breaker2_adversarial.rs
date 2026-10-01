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
const ACTOR: u32 = 0x812;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

struct DataDir(PathBuf);

impl DataDir {
    fn new(chance: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("mod01-breaker2-{}-{chance}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("Seed_DISTR.ini"),
            format!("Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|{chance}\n"),
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

fn game() -> Game {
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
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0u8; 24]),
    );
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

#[test]
fn mod_01_save_character_id_keeps_all_64_bits() {
    let game = game();
    for character_id in [0, 0x2A73_F01A, u64::MAX] {
        game.world_mut().set_player_character_id(character_id);
        assert_eq!(game.world_mut().player_character_id(), character_id);
    }
}

#[test]
fn mod_01_deterministic_chance_boundaries_are_exact() {
    for (chance, expected) in [("0!", false), ("100!", true)] {
        let data = DataDir::new(chance);
        let mut game = game();
        game.world_mut().set_player_character_id(0x2A73_F01A);
        game.apply_mod_data(std::slice::from_ref(&data.0));
        let received = game
            .world_mut()
            .actor(id(ACTOR))
            .unwrap()
            .spells
            .contains(&id(SPELL));
        assert_eq!(received, expected, "SPID chance {chance} boundary");
    }
}

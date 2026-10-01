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
const REF: u32 = 0x812;
fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}
struct Dir(PathBuf);
impl Dir {
    fn new(case: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("mod01-adversarial-{}-{case}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn game(extra: u32) -> Game {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(T(b"SPEL"), SPELL).zsub(T(b"EDID"), "SeedSpell"));
    p.push_top(npc::build_record(
        BASE,
        "SeedBase",
        "Seed Base",
        Config::default(),
        Stats::default(),
    ));
    for n in 0..=extra {
        let object = REF + n;
        p.push_top(
            Record::new(T(b"ACHR"), object)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0u8; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&p.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}
#[test]
fn mod_01_actor_seed_is_stable_per_reference_even_when_built_repeatedly() {
    let d = Dir::new("variation");
    fs::write(
        d.0.join("Seed_DISTR.ini"),
        "Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|50!\n",
    )
    .unwrap();
    let mut g = game(24);
    g.apply_mod_data(&d.roots());
    let first: Vec<_> = (0..=24)
        .map(|n| {
            g.world_mut()
                .actor(id(REF + n))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    let repeated: Vec<_> = (0..=24)
        .map(|n| {
            g.world_mut()
                .actor(id(REF + n))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    assert_eq!(
        first, repeated,
        "SPID deterministic actor choices must not reroll on repeated actor access"
    );
    assert!(
        first.iter().any(|v| *v) && first.iter().any(|v| !*v),
        "same-base references must receive distinct per-actor choices: {first:?}"
    );
}

#[test]
fn mod_01_deterministic_actor_results_use_save_character_id() {
    let d = Dir::new("save-character-id");
    fs::write(
        d.0.join("Seed_DISTR.ini"),
        "Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|50!\n",
    )
    .unwrap();
    let results = |character_id| {
        let mut g = game(24);
        g.world_mut().set_player_character_id(character_id);
        assert_eq!(g.world_mut().player_character_id(), character_id);
        g.apply_mod_data(&d.roots());
        (0..=24)
            .map(|n| {
                g.world_mut()
                    .actor(id(REF + n))
                    .unwrap()
                    .spells
                    .contains(&id(SPELL))
            })
            .collect::<Vec<_>>()
    };
    let save_id = results(0x2A73_F01A);
    let repeat = results(0x2A73_F01A);
    let other_save_id = results(0x19B4_7C20);
    assert_eq!(save_id, repeat, "one save identity repeats actor choices");
    assert_ne!(
        save_id, other_save_id,
        "different save character IDs produce the source-defined seed inputs"
    );
    assert!(save_id.iter().any(|result| *result) && save_id.iter().any(|result| !*result));
}
#[test]
fn mod_01_deterministic_rule_keeps_base_filter_and_rejects_missing_actor() {
    let d = Dir::new("missing");
    fs::write(
        d.0.join("Seed_DISTR.ini"),
        "Spell = MissingSpell|SeedBase|NONE|NONE|NONE|1|100!\n",
    )
    .unwrap();
    let mut g = game(0);
    let report = g.apply_mod_data(&d.roots());
    assert_eq!(
        report.distributions_applied, 0,
        "deterministic rules must respect SPID base filters"
    );
    assert!(!g
        .world_mut()
        .actor(id(REF))
        .unwrap()
        .spells
        .contains(&id(SPELL)));
    assert!(
        g.world_mut().actor(id(0xdead)).is_none(),
        "an unresolved actor reference cannot receive a distribution"
    );
}

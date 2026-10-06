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
fn mod_01_actor_choices_match_spid_seed_and_rng_oracle() {
    let d = Dir::new("upstream-oracle");
    let entry = "SeedSpell|SeedBase|NONE|NONE|NONE|1|50!";
    fs::write(d.0.join("Seed_DISTR.ini"), format!("Spell = {entry}\n")).unwrap();
    let character_id = 0x2A73_F01A;
    let mut g = game(24);
    g.world_mut().set_player_character_id(character_id);
    g.apply_mod_data(&d.roots());

    let actual: Vec<_> = (0..=24)
        .map(|offset| {
            let actor = id(REF + offset);
            let runtime_id = u64::from(g.world().order().runtime_id(actor));
            let expected = spid_oracle(entry, character_id, runtime_id, 50.0);
            let received = g
                .world_mut()
                .actor(actor)
                .unwrap()
                .spells
                .contains(&id(SPELL));
            assert_eq!(
                received, expected,
                "actor {actor:?}, runtime ID {runtime_id:#x}"
            );
            received
        })
        .collect();
    assert!(actual.iter().any(|choice| *choice));
    assert!(actual.iter().any(|choice| !*choice));
}

// Independently transcribes SPID's hash_combine and CLibUtil's seeded
// Xoshiro256** draw, then compares the actor's applied choice to that oracle.
fn spid_oracle(entry: &str, character_id: u64, actor_id: u64, chance: f64) -> bool {
    let line_seed = entry.bytes().fold(0xcbf2_9ce4_8422_2325, |seed, byte| {
        (seed ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let mut seed = line_seed;
    for value in [character_id, actor_id] {
        seed ^= value
            .wrapping_add(0x9e37_79b9)
            .wrapping_add(seed << 6)
            .wrapping_add(seed >> 2);
    }
    let mut state = seed;
    let mut words = [0_u64; 4];
    for word in &mut words {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        *word = value ^ (value >> 31);
    }
    let result = words[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let unit = (result >> 11) as f64 / (1_u64 << 53) as f64;
    unit * 100.0 <= chance
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

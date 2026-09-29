// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Planned check for PRG-15: "Source-grounded skill calculation matches
//! manual and automatic actors with winning race and class overrides
//! including offsets rounding and caps."
//!
//! Sources: `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` (CLAS and
//! RACE DATA layouts, ~lines 4303 and 9298), `engine-refs/uesp-modfileformat/
//! CLAS.wikitext` ("Weight Calculations": `Skill = 15 + Racial bonus +
//! 8*(Level-1)/(Sum of weights)*Weight`, complete sets by floor division,
//! leftover points by weight then actor value order, capped at each skill's
//! own weight), `docs/actor-values.md`. Fixtures mirror
//! `crates/game/tests/actor_values.rs`'s `race()` helper and
//! `crates/game/tests/actor_skills_adversarial.rs`'s `clas()` helper, with
//! independently chosen numbers so the two files do not simply repeat each
//! other's arithmetic.

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Npc, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

/// A RACE record with up to 7 `(skill, boost)` pairs at DATA bytes 0 to 13
/// and the finite attribute floats `Attributes::from_record` requires from
/// byte 36 onward (same layout `actor_values.rs::race` and
/// `actor_skills_adversarial.rs::race_with_boosts` already use).
fn race(raw: u32, boosts: &[(i8, i8)]) -> Record {
    let mut bytes = vec![0u8; 128];
    for (i, &(skill, boost)) in boosts.iter().enumerate() {
        bytes[i * 2] = skill as u8;
        bytes[i * 2 + 1] = boost as u8;
    }
    for (offset, value) in [
        (36, 100.0f32),
        (40, 50.0),
        (44, 60.0),
        (48, 300.0),
        (52, 1.0),
        (84, 0.7),
        (88, 3.0),
        (92, 5.0),
        (96, 4.0),
        (100, 64.0),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(T(b"RACE"), raw).sub(T(b"DATA"), bytes)
}

/// A CLAS record with the given per-skill weights (`npc::SKILLS` order),
/// the 35 byte minimum DATA the parser reads.
fn clas(raw: u32, weights: [u8; 18]) -> Record {
    let mut data = vec![0u8; 35];
    data[6..24].copy_from_slice(&weights);
    Record::new(T(b"CLAS"), raw).sub(T(b"DATA"), data)
}

fn skill_value(values: &[(String, f32)], skill: &str) -> f32 {
    values
        .iter()
        .find(|(name, _)| name == skill)
        .unwrap_or_else(|| panic!("missing skill {skill} in computed actor values"))
        .1
}

// Manual DNAM skill values may already include race boosts. Preserve the
// authored cache until the manual-mode calculation is established.
// Local skill index 5 is HeavyArmor, full actor value 11.
#[test]
fn prg_15_manual_actor_skill_preserves_the_dnam_cache() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, &[(11, 15)]));
    let mut stats = Stats::default();
    stats.skills[5] = 40;
    plugin.push_top(
        npc::build_record(0x800, "Guard", "Guard", Config::default(), stats)
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "heavyarmor"),
        40.0,
        "manual DNAM skill cache must not receive a second race boost"
    );
}

// Clause: "automatic actors ... including ... rounding". UESP CLAS.wikitext
// assigns complete sets by `floor(8*(Level-1)/Sum of weights)*Weight`. One
// weighted skill (Lockpicking, weight 3) at level 5: points = 8*4 = 32,
// 32/3 truncates to 10 (not rounds to 11), remainder 2 goes to the same
// skill as leftover, total 32 = 10*3 + 2, skill = 15 + 0 + 30 + 2 = 47.
// A naive round-to-nearest division (11) would instead read 15+33=48.
#[test]
fn prg_15_automatic_actor_skill_formula_truncates_the_complete_set_division() {
    let mut weights = [0u8; 18];
    weights[8] = 3; // Lockpicking
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, &[]));
    plugin.push_top(clas(0x900, weights));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Thief",
            "Thief",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 5,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "lockpicking"),
        47.0,
        "floor(32/3)=10 complete sets plus 2 leftover must read 47, not 48 from a rounded division"
    );
}

// Clause: "automatic actors ... including ... caps", both directions. A
// heavily weighted skill at a high level must clamp at 100, not overflow
// past it, and an unweighted skill with a large negative racial bonus must
// clamp at 0, not go negative, in the same computed actor.
#[test]
fn prg_15_automatic_actor_skill_clamps_at_both_bounds() {
    let mut weights = [0u8; 18];
    weights[6] = 1; // LightArmor, full actor value 12: weighted, will overflow past 100.
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, &[(12, 0), (18, -90)])); // Alteration (idx 12, AV 18): unweighted, deep negative bonus.
    plugin.push_top(clas(0x900, weights));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Veteran",
            "Veteran",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 20,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "lightarmor"),
        100.0,
        "15 + 8*19 = 167 must clamp at 100, not overflow"
    );
    assert_eq!(
        skill_value(&values, "alteration"),
        0.0,
        "15 - 90 = -75 must clamp at 0, not go negative"
    );
}

// Clause: "winning race and class overrides", both together in one actor,
// not each in isolation. A one-master patch overrides both the RACE and the
// CLAS the NPC's own file defines. The skill must reflect the winning
// definitions of both, not either losing one.
#[test]
fn prg_15_winning_race_and_class_overrides_both_apply_together() {
    let mut base = Plugin::new(FileHeader::default());
    let mut losing_weights = [0u8; 18];
    losing_weights[10] = 1; // Alchemy: losing weight.
    base.push_top(race(0x700, &[(16, 2)])); // Alchemy (AV 16): losing boost.
    base.push_top(clas(0x900, losing_weights));

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    let mut winning_weights = [0u8; 18];
    winning_weights[10] = 4; // Alchemy: winning weight.
    patch.push_top(race(0x700, &[(16, 9)])); // Alchemy: winning boost.
    patch.push_top(clas(0x900, winning_weights));
    patch.push_top(
        npc::build_record(
            0x800,
            "Alchemist",
            "Alchemist",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 6,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    // Winning: total_weight=4, points=8*5=40, per_set=10, remainder=0,
    // skill = 15 + 9 + 40 = 64. The losing definitions (weight 1, boost 2)
    // would instead read 15 + 2 + 40 = 57.
    assert_eq!(
        skill_value(&values, "alchemy"),
        64.0,
        "both the winning race boost and the winning class weight must apply, not the losing ones"
    );
}

// Vanilla grounding: computed skills stay within 0 to 100. Authored manual
// DNAM skill caches may exceed 100 and are checked for finite values only.
// Skipped without SKYRIM_DATA, following
// `actor_values_live.rs`'s own gate.
#[test]
fn prg_15_vanilla_manual_and_automatic_actors_stay_in_bounds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping prg_15 vanilla actor survey");
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
    let world = game.world();
    let mut manual = 0usize;
    let mut class_formula_eligible = 0usize;
    for (npc_id, record) in world
        .order()
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, r)| !r.is_deleted())
    {
        let npc = Npc::from_record(record, false).unwrap();
        let Ok(values) = world.initial_actor_values(npc_id) else {
            continue;
        };
        for name in npc::SKILLS {
            let key = name.to_ascii_lowercase();
            let value = skill_value(&values, &key);
            assert!(value.is_finite(), "{npc_id}.{key} is not finite: {value}");
        }
        let autocalc = npc.config.flags & npc::flags::AUTO_CALC_STATS != 0;
        let pc_level_mult = npc.config.flags & npc::flags::PC_LEVEL_MULT != 0;
        if !autocalc {
            manual += 1;
        } else if !pc_level_mult {
            if let Some(raw) = npc.class.filter(|raw| *raw != 0) {
                if let Some(class_id) = world.resolve_from(npc_id, raw) {
                    if world
                        .record(class_id)
                        .is_some_and(|r| !r.is_deleted() && r.tag == Tag(*b"CLAS"))
                    {
                        class_formula_eligible += 1;
                        for name in npc::SKILLS {
                            let key = name.to_ascii_lowercase();
                            let value = skill_value(&values, &key);
                            assert!(
                                (0.0..=100.0).contains(&value),
                                "{npc_id}.{key} computed out of bounds: {value}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        manual > 100,
        "expected many manual actors in the vanilla survey, got {manual}"
    );
    assert!(
        class_formula_eligible > 50,
        "expected the class skill formula to be eligible for many real auto-calculated NPCs, got {class_formula_eligible}"
    );
}

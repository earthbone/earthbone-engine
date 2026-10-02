// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for PRG-15 (`crates/game/src/world/actor_seeds.rs`,
//! `crates/game/src/class.rs`, `crates/game/src/race.rs`): the skill
//! computation the writer added had zero prior test coverage in this file.
//! Sources: `engine-refs/uesp-modfileformat/CLAS.wikitext` (Weight
//! Calculations), `engine-refs/uesp-modfileformat/RACE.wikitext` (Skill
//! Boosts), `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` (CLAS and RACE
//! DATA layouts), and the PRG-15 done check itself: "Source-grounded skill
//! calculation matches manual and automatic actors with winning race and
//! class overrides including offsets rounding and caps".

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    class::Class,
    npc::{self, Config, Stats},
    race::Attributes,
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

/// A minimal RACE record whose DATA carries up to 7 `(skill, boost)` pairs
/// at bytes 0 to 13 plus the finite attribute floats the parser requires
/// from byte 36 onward. Mirrors `crates/game/tests/actor_values.rs`'s own
/// `race()` helper so the fixed attribute floats stay realistic.
fn race_with_boosts(raw: u32, boosts: &[(i8, i8)]) -> Record {
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

/// A CLAS record with the given per-skill weights (`crate::npc::SKILLS`
/// order) and the 35 byte minimum DATA the parser requires: 4 unknown bytes,
/// teaches, maximum training level, 18 weights, bleedout, voice points and
/// the 3 attribute weights it actually reads.
fn clas(raw: u32, weights: [u8; 18]) -> Record {
    let mut data = vec![0u8; 35];
    data[4] = 0xFF; // teaches: none
    data[5] = 0; // maximum training level
    data[6..24].copy_from_slice(&weights);
    // bytes 24..28 bleedout, 28..32 voice points, left zero.
    data[32] = 10; // health weight
    data[33] = 0; // magicka weight
    data[34] = 0; // stamina weight
    Record::new(T(b"CLAS"), raw).sub(T(b"DATA"), data)
}

fn skill_value(values: &[(String, f32)], skill: &str) -> f32 {
    values
        .iter()
        .find(|(name, _)| name == skill)
        .unwrap_or_else(|| panic!("missing skill {skill} in computed actor values"))
        .1
}

// 1. CLAS DATA's minimum length is 35 bytes: the highest field the parser
// reads is the stamina attribute weight at byte 34. `docs/style.md`'s own
// contract on `Class::from_record` is "Rejects a wrong record type or short
// DATA", and TES5Edit's CLAS definition (wbDefinitionsTES5.pas ~line 4303)
// lays out no field past byte 34 that this reader consumes. 35 bytes must
// parse, 34 bytes must fail cleanly rather than panic on an out of bounds
// index.
#[test]
fn class_data_parses_at_its_documented_minimum_and_rejects_one_byte_short() {
    let ok = Record::new(T(b"CLAS"), 1).sub(T(b"DATA"), vec![0u8; 35]);
    assert!(
        Class::from_record(&ok).is_ok(),
        "35 bytes is the documented minimum CLAS DATA length"
    );
    let short = Record::new(T(b"CLAS"), 1).sub(T(b"DATA"), vec![0u8; 34]);
    assert!(
        Class::from_record(&short).is_err(),
        "34 bytes must not silently read a byte past the subrecord"
    );
}

// 2. RACE DATA's 7 skill/boost pairs (uesp RACE.wikitext, "Skill Boosts";
// TES5Edit wbDefinitionsTES5.pas ~line 9298, both fields itS8) must land in
// `Attributes::skill_boosts` in file order and byte order (skill first,
// boost second), not swapped or reordered. Each pair uses a distinguishable
// skill index and boost so a transposition anywhere in the 7 slots shows up.
#[test]
fn race_skill_boosts_preserve_slot_order_and_signed_byte_order() {
    let expected: [(i8, i8); 7] = [
        (6, 10),
        (7, -20),
        (10, 30),
        (12, -40),
        (15, 50),
        (20, -60),
        (23, 70),
    ];
    let record = race_with_boosts(1, &expected);
    let attributes = Attributes::from_record(&record).unwrap().unwrap();
    assert_eq!(
        attributes.skill_boosts, expected,
        "RACE DATA skill boosts must decode in slot and byte order"
    );
}

// 3. CLAS DATA's 18 skill weights (bytes 6 to 23) must land in
// `crate::npc::SKILLS` order, per this repo's own doc comment on
// `class::Class::skill_weights` and TESClass.h's `skillWeights[18]`. A
// distinct weight per index catches any off-by-one or reversed copy.
#[test]
fn class_skill_weights_preserve_the_skills_array_order() {
    let mut weights = [0u8; 18];
    for (i, w) in weights.iter_mut().enumerate() {
        *w = (i + 1) as u8;
    }
    let record = clas(1, weights);
    let class = Class::from_record(&record).unwrap().unwrap();
    assert_eq!(
        class.skill_weights, weights,
        "CLAS skill weights must decode in crate::npc::SKILLS order"
    );
}

// 4/5. Manual DNAM skill values may already contain the race boost. Keep the
// authored cache until the source contract establishes how to recompute it.
#[test]
fn manual_actor_skill_preserves_high_dnam_cache_without_another_race_boost() {
    let mut plugin = Plugin::new(FileHeader::default());
    // Local skill index 14 (Destruction, full actor value 20).
    plugin.push_top(race_with_boosts(0x700, &[(20, 50)]));
    let mut stats = Stats::default();
    stats.skills[14] = 90;
    plugin.push_top(
        npc::build_record(0x800, "Caster", "Caster", Config::default(), stats)
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "destruction"),
        90.0,
        "manual DNAM skill cache must not receive a second race boost"
    );
}

#[test]
fn manual_actor_skill_preserves_low_dnam_cache_without_another_race_boost() {
    let mut plugin = Plugin::new(FileHeader::default());
    // Local skill index 14 (Destruction, full actor value 20).
    plugin.push_top(race_with_boosts(0x700, &[(20, -60)]));
    let mut stats = Stats::default();
    stats.skills[14] = 10;
    plugin.push_top(
        npc::build_record(0x800, "Caster", "Caster", Config::default(), stats)
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "destruction"),
        10.0,
        "manual DNAM skill cache must not receive a second race boost"
    );
}

// 6. The full UESP CLAS.wikitext "Weight Calculations" formula for a fixed
// level auto-calculated actor, with two skills of distinct (so tie-break
// free) weights, a racial boost on the lower-weight skill and on an
// unweighted skill, and level 3. total_weight = 5 + 2 = 7,
// points = 8*(3-1) = 16, per_set = 16/7 = 2, remainder = 2, both skills
// under their own weight so each gets exactly one leftover point with no
// order ambiguity: Smithing (weight 5) = 15+0+2*5+1 = 26, Alchemy
// (weight 2, +8 racial) = 15+8+2*2+1 = 28, OneHanded (weight 0, -5 racial)
// = 15-5+0+0 = 10.
#[test]
fn autocalculated_skill_formula_matches_weights_racial_boost_and_leftover_points() {
    let mut weights = [0u8; 18];
    weights[4] = 5; // Smithing
    weights[10] = 2; // Alchemy
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race_with_boosts(
        0x700,
        &[
            (6, -5), // OneHanded, full AV 6, weight 0
            (16, 8), // Alchemy, full AV 16, weight 2
        ],
    ));
    plugin.push_top(clas(0x900, weights));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Student",
            "Student",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 3,
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
    assert_eq!(skill_value(&values, "smithing"), 26.0);
    assert_eq!(skill_value(&values, "alchemy"), 28.0);
    assert_eq!(
        skill_value(&values, "onehanded"),
        10.0,
        "an unweighted skill still gets 15 plus its racial boost"
    );
}

// 7. `autocalculated_dnam_cache_is_not_given_the_offset_twice` in
// `actor_values.rs` covers health/magicka/stamina under PC_LEVEL_MULT. This
// mirrors it for skills: `actor_seeds.rs`'s own doc comment states skills
// keep DNAM's cache under PC_LEVEL_MULT, "whose player-scaled level this
// store does not track", even when a resolvable class is present.
#[test]
fn pc_level_mult_autocalc_actor_keeps_the_dnam_skill_cache_despite_a_resolvable_class() {
    let mut weights = [0u8; 18];
    weights[4] = 5; // Smithing: would dominate the formula if it ran.
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(clas(0x900, weights));
    let mut stats = Stats::default();
    stats.skills[4] = 37;
    plugin.push_top(
        npc::build_record(
            0x800,
            "Scaled",
            "Scaled",
            Config {
                flags: npc::flags::AUTO_CALC_STATS | npc::flags::PC_LEVEL_MULT,
                level: 1000,
                ..Config::default()
            },
            stats,
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "smithing"),
        37.0,
        "PC_LEVEL_MULT actors must keep the DNAM skill cache, the class formula must not run"
    );
}

// 8. A winning CLAS override in a later plugin must be the class the skill
// formula reads, not the base master's losing definition. Construction
// follows `crates/game/tests/perks.rs`'s
// `winning_override_links_use_the_winning_plugins_master_order`: a
// single-master patch's raw FormID with master index 0 resolves to the same
// global id its one master owns, per `resolve_in` in
// `crates/esm/src/load_order.rs`. Done check: "winning race and class
// overrides".
#[test]
fn winning_class_override_supplies_the_skill_weights_not_the_losing_definition() {
    let mut base_weights = [0u8; 18];
    base_weights[0] = 3; // OneHanded, in the losing base definition only.
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(clas(0x900, base_weights));
    let mut patch_weights = [0u8; 18];
    patch_weights[10] = 3; // Alchemy, in the winning override.
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(clas(0x900, patch_weights));
    patch.push_top(
        npc::build_record(
            0x800,
            "Patched",
            "Patched",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 5,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "alchemy"),
        47.0,
        "level 5: 15 + 0 + 10*3 + 2 leftover, from the winning class's weight"
    );
    assert_eq!(
        skill_value(&values, "onehanded"),
        15.0,
        "the losing base class's weight on OneHanded must not apply"
    );
}

// 9. An auto-calculated actor reads the winning RACE skill boost.
#[test]
fn winning_race_override_supplies_the_skill_boost_not_the_losing_definition() {
    let mut base = Plugin::new(FileHeader::default());
    // Local skill index 4 (Smithing, full actor value 10).
    base.push_top(race_with_boosts(0x700, &[(10, 5)]));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(race_with_boosts(0x700, &[(10, 30)]));
    let mut weights = [0; 18];
    weights[4] = 1;
    patch.push_top(clas(0x900, weights));
    let mut stats = Stats::default();
    stats.skills[4] = 20;
    patch.push_top(
        npc::build_record(
            0x800,
            "Smith",
            "Smith",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 1,
                ..Config::default()
            },
            stats,
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        skill_value(&values, "smithing"),
        45.0,
        "15 base + 30 winning race boost, the losing +5 must not apply"
    );
}

// 10. `npc_class`'s own doc comment states a `CNAM` landing on the wrong
// record type is "not an error", falling back to DNAM's cache. A CNAM
// pointing at the actor's own RACE record (present, resolvable, wrong tag)
// must not error and must not use that record's bytes as class data.
#[test]
fn cnam_pointing_at_a_non_class_record_falls_back_to_the_dnam_cache() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race_with_boosts(0x700, &[]));
    let mut stats = Stats::default();
    stats.skills[4] = 33;
    plugin.push_top(
        npc::build_record(
            0x800,
            "Confused",
            "Confused",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 10,
                ..Config::default()
            },
            stats,
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game
        .world()
        .initial_actor_values(id(0, 0x800))
        .expect("a CNAM on a wrong-tag record must not surface as a store error");
    assert_eq!(
        skill_value(&values, "smithing"),
        33.0,
        "no resolvable class means the DNAM skill cache is kept"
    );
}

// 11. `actor_values_from_sources`'s own comment states a class that "fails
// to ... parse is not a store error". A resolvable CLAS record whose DATA
// is one byte short of the 35 byte minimum must fall back to the DNAM
// cache, not propagate `Class::from_record`'s `Err` up through
// `initial_actor_values`.
#[test]
fn malformed_short_class_data_falls_back_to_the_dnam_cache_instead_of_erroring() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CLAS"), 0x900).sub(T(b"DATA"), vec![0u8; 34]));
    let mut stats = Stats::default();
    stats.skills[4] = 44;
    plugin.push_top(
        npc::build_record(
            0x800,
            "Malformed",
            "Malformed",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 10,
                ..Config::default()
            },
            stats,
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game
        .world()
        .initial_actor_values(id(0, 0x800))
        .expect("a malformed CLAS DATA must not surface as a store error");
    assert_eq!(
        skill_value(&values, "smithing"),
        44.0,
        "malformed class data means the DNAM skill cache is kept"
    );
}

// 12. A deleted CLAS record must be treated as unresolved, the same as a
// missing one, never read for its stale subrecord bytes. `npc_class`
// explicitly rejects `record.is_deleted()`.
#[test]
fn deleted_class_record_falls_back_to_the_dnam_cache_instead_of_stale_data() {
    let mut weights = [0u8; 18];
    weights[4] = 5;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(clas(0x900, weights).with_flags(soe_esm::plugin::flags::DELETED));
    let mut stats = Stats::default();
    stats.skills[4] = 55;
    plugin.push_top(
        npc::build_record(
            0x800,
            "Deleted",
            "Deleted",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 10,
                ..Config::default()
            },
            stats,
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game
        .world()
        .initial_actor_values(id(0, 0x800))
        .expect("a deleted CLAS record must not surface as a store error");
    assert_eq!(
        skill_value(&values, "smithing"),
        55.0,
        "a deleted class record must not be read for its stale weights"
    );
}

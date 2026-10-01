// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for PRG-03 (`crates/game/src/world/actor_seeds.rs`'s
//! `class_attribute_values`): zero prior test coverage of the new function
//! per the writer's own reported gap. Mirrors
//! `crates/game/tests/actor_skills_adversarial.rs`'s fixtures and adversarial
//! patterns for the sibling skill formula, with independently chosen numbers.
//!
//! Sources: `engine-refs/uesp-modfileformat/CLAS.wikitext` ("Weight
//! Calculations" / "Attributes": the formula is racial bonus, plus NPC
//! offset, plus 10 times Level minus 1 divided by the sum of weights, times
//! the attribute weight. Health additionally gets 5 times Level minus 1,
//! "independent of its weight". Leftover points are ordered "first in
//! decreasing order of weight" then, on a tie, "reverse actor value index
//! order (stamina, magicka, then health)"),
//! `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` (CLAS and RACE DATA
//! layouts), and the PRG-03 done check: "Class and level fixtures derive
//! health, magicka and stamina from records instead of cached hand-supplied
//! values".

use crate::common::installed;

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

/// A RACE record whose DATA carries the finite attribute floats
/// `Attributes::from_record` requires from byte 36 onward, with health,
/// magicka and stamina as the only fields these tests vary. Mirrors
/// `crates/game/tests/actor_values.rs`'s own `race()` helper.
fn race(raw: u32, health: f32, magicka: f32, stamina: f32) -> Record {
    let mut bytes = vec![0u8; 128];
    for (offset, value) in [
        (36, health),
        (40, magicka),
        (44, stamina),
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

/// A CLAS record with all 18 skill weights zero and the given
/// `[health, magicka, stamina]` attribute weights at DATA bytes 32 to 34,
/// the 35 byte minimum the parser requires.
fn clas(raw: u32, attribute_weights: [u8; 3]) -> Record {
    let mut data = vec![0u8; 35];
    data[32..35].copy_from_slice(&attribute_weights);
    Record::new(T(b"CLAS"), raw).sub(T(b"DATA"), data)
}

fn attr_value(values: &[(String, f32)], attribute: &str) -> f32 {
    values
        .iter()
        .find(|(name, _)| name == attribute)
        .unwrap_or_else(|| panic!("missing actor value {attribute} in computed actor values"))
        .1
}

// 1. The PRG-03 done check itself: an AUTO_CALC_STATS actor with a
// resolvable class must derive health, magicka and stamina from the class
// weights, race start and level, not from the NPC's own cached DNAM Stats
// block. The DNAM block here is stuffed with an implausible decoy (9999) so
// any code path that still reads it produces an unmistakably wrong number.
// total_weight = 2+1+1 = 4, level 5: points = 10*4 = 40, per_set = 10,
// remainder = 0. health = 100 + 5*4 + 10*2 + 20(offset) = 160.
// magicka = 50 + 10*1 - 10(offset) = 50. stamina = 60 + 10*1 + 15(offset) = 85.
#[test]
fn autocalc_actor_attributes_use_the_class_formula_not_the_cached_dnam_values() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 100.0, 50.0, 60.0));
    plugin.push_top(clas(0x900, [2, 1, 1]));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Formula",
            "Formula",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 5,
                health_offset: 20,
                magicka_offset: -10,
                stamina_offset: 15,
                ..Config::default()
            },
            Stats {
                health: 9999,
                magicka: 9999,
                stamina: 9999,
                ..Stats::default()
            },
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        attr_value(&values, "health"),
        160.0,
        "the class formula, not the 9999 DNAM cache, must supply health"
    );
    assert_eq!(
        attr_value(&values, "magicka"),
        50.0,
        "the class formula, not the 9999 DNAM cache, must supply magicka"
    );
    assert_eq!(
        attr_value(&values, "stamina"),
        85.0,
        "the class formula, not the 9999 DNAM cache, must supply stamina"
    );
}

// 2. UESP CLAS.wikitext, Attributes: "health always receives an additional
// 5 point/level increase, independent of its weight". A class with a health
// weight of 0 must still grant the flat level bonus to health, it must not
// gate the bonus on health having a nonzero share of the weighted pool.
// total_weight = 0+3+1 = 4, level 3: points = 10*2 = 20, per_set = 5,
// remainder = 0. health = 100 + 5*2 + 5*0 = 110 (not 100).
#[test]
fn health_receives_its_level_bonus_even_when_the_class_gives_it_no_weight() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 100.0, 50.0, 60.0));
    plugin.push_top(clas(0x900, [0, 3, 1]));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Unweighted",
            "Unweighted",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 3,
                ..Config::default()
            },
            Stats {
                health: 1,
                magicka: 1,
                stamina: 1,
                ..Stats::default()
            },
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        attr_value(&values, "health"),
        110.0,
        "a zero health weight must not suppress the unconditional 5*(Level-1) bonus"
    );
    assert_eq!(attr_value(&values, "magicka"), 65.0);
    assert_eq!(attr_value(&values, "stamina"), 65.0);
}

// 3. UESP CLAS.wikitext, Attributes: on a weight tie, leftover points go "in
// reverse actor value index order (stamina, magicka, then health)". Three
// equally weighted attributes with a remainder of 2 must give the leftover
// to stamina and magicka, never to health, which a plain index-ascending
// tie-break (the skill formula's own rule) would instead prefer.
// total_weight = 3, level 3: points = 10*2 = 20, per_set = 6, remainder = 2.
// Correct: stamina and magicka each +1, health +0.
// health = 0 + 5*2 + 6*1 + 0 = 16. magicka = 6+1 = 7. stamina = 6+1 = 7.
// An ascending tie-break would instead give health the first leftover point
// (17, 7, 6).
#[test]
fn leftover_attribute_points_go_to_stamina_before_magicka_before_health_on_a_weight_tie() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(clas(0x900, [1, 1, 1]));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Tied",
            "Tied",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 3,
                ..Config::default()
            },
            Stats {
                health: 1,
                magicka: 1,
                stamina: 1,
                ..Stats::default()
            },
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        attr_value(&values, "stamina"),
        7.0,
        "stamina is first in the tie-break order and must take a leftover point"
    );
    assert_eq!(
        attr_value(&values, "magicka"),
        7.0,
        "magicka is second in the tie-break order and must take the other leftover point"
    );
    assert_eq!(
        attr_value(&values, "health"),
        16.0,
        "health is last in the tie-break order and must not receive a leftover point here"
    );
}

// 4. The leftover loop must wrap through more than one round when a small
// weight's cap is reached before the remainder is exhausted, and must
// respect each attribute's own weight as its cap while doing so. Weights
// [1, 1, 5] (health, magicka, stamina), level 7: total_weight = 7,
// points = 10*6 = 60, per_set = 8, remainder = 4. Tie-break order is
// stamina (weight 5) first, then magicka before health (both weight 1, tied,
// reverse index). Round 1 gives each of the 3 slots one point (stamina 1/5,
// magicka 1/1 now capped, health 1/1 now capped), leaving 1 point that only
// stamina (not yet capped) can take in round 2.
// health = 100 + 5*6 + 8*1 + 1 = 139. magicka = 50 + 8*1 + 1 = 59.
// stamina = 60 + 8*5 + 2 = 102.
#[test]
fn leftover_attribute_distribution_wraps_through_a_second_round_when_a_weight_is_small() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 100.0, 50.0, 60.0));
    plugin.push_top(clas(0x900, [1, 1, 5]));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Wrapped",
            "Wrapped",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 7,
                ..Config::default()
            },
            Stats {
                health: 7,
                magicka: 7,
                stamina: 7,
                ..Stats::default()
            },
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(attr_value(&values, "health"), 139.0);
    assert_eq!(attr_value(&values, "magicka"), 59.0);
    assert_eq!(
        attr_value(&values, "stamina"),
        102.0,
        "stamina's own weight (5) must let it absorb the second round's leftover point, \
         a one-pass-only distribution or a missed cap check would both misplace it"
    );
}

// 5. Winning race and class overrides must both apply to the attribute
// formula, not either losing definition, mirroring
// `actor_skills_adversarial.rs`'s equivalent skill test for the sibling
// formula. A one-master patch overrides the NPC's own file's RACE and CLAS.
// Winning: total_weight = 3+1 = 4, level 5: points = 40, per_set = 10,
// remainder = 0. health = 400 + 5*4 + 10*3 = 450. magicka = 50 + 10*1 = 60.
// The losing definitions (race health 100, weights [1, 3, 0]) would instead
// read health = 100 + 20 + 10 = 130, magicka = 50 + 30 = 80.
#[test]
fn winning_class_and_race_overrides_both_apply_to_the_attribute_formula() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(race(0x700, 100.0, 50.0, 60.0));
    base.push_top(clas(0x900, [1, 3, 0]));

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(race(0x700, 400.0, 50.0, 60.0));
    patch.push_top(clas(0x900, [3, 1, 0]));
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
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        attr_value(&values, "health"),
        450.0,
        "the winning race start and the winning class weight must both apply to health, \
         not the losing race's 100 or the losing class's weight 1"
    );
    assert_eq!(
        attr_value(&values, "magicka"),
        60.0,
        "the winning class weight must apply to magicka, not the losing class's weight 3"
    );
}

// 6. `actor_skills_adversarial.rs`'s
// `pc_level_mult_autocalc_actor_keeps_the_dnam_skill_cache_despite_a_resolvable_class`
// covers skills under PC_LEVEL_MULT. This mirrors it for attributes:
// `actor_seeds.rs` gates the whole class block, skills and attributes alike,
// behind `PC_LEVEL_MULT == 0`, because this store does not track the
// player's scaled level. A resolvable class with dominant weights, and a
// nonzero NPC offset, must not move health, magicka or stamina off the raw
// DNAM cache.
#[test]
fn pc_level_mult_autocalc_actor_keeps_the_dnam_attribute_cache_despite_a_resolvable_class() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(clas(0x900, [5, 5, 5]));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Scaled",
            "Scaled",
            Config {
                flags: npc::flags::AUTO_CALC_STATS | npc::flags::PC_LEVEL_MULT,
                level: 1000,
                health_offset: 50,
                magicka_offset: 50,
                stamina_offset: 50,
                ..Config::default()
            },
            Stats {
                health: 222,
                magicka: 333,
                stamina: 444,
                ..Stats::default()
            },
        )
        .sub(T(b"CNAM"), 0x900u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        [
            attr_value(&values, "health"),
            attr_value(&values, "magicka"),
            attr_value(&values, "stamina"),
        ],
        [222.0, 333.0, 444.0],
        "PC_LEVEL_MULT actors must keep the raw DNAM attribute cache, unoffset, \
         the class formula must not run"
    );
}

// 7. `npc_class`'s own doc comment states a `CNAM` landing on the wrong
// record type is "not an error", falling back to DNAM's cache. Mirrors
// `actor_skills_adversarial.rs`'s equivalent skill test for the new
// attribute path.
#[test]
fn cnam_pointing_at_a_non_class_record_keeps_the_dnam_attribute_cache() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 100.0, 50.0, 60.0));
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
            Stats {
                health: 111,
                magicka: 222,
                stamina: 333,
                ..Stats::default()
            },
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
        [
            attr_value(&values, "health"),
            attr_value(&values, "magicka"),
            attr_value(&values, "stamina"),
        ],
        [111.0, 222.0, 333.0],
        "no resolvable class means the DNAM attribute cache is kept"
    );
}

// 8. `actor_values_from_sources`'s own comment states a class that "fails to
// ... parse is not a store error". Mirrors
// `actor_skills_adversarial.rs`'s equivalent skill test for the new
// attribute path.
#[test]
fn malformed_short_class_data_keeps_the_dnam_attribute_cache_instead_of_erroring() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CLAS"), 0x900).sub(T(b"DATA"), vec![0u8; 34]));
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
            Stats {
                health: 101,
                magicka: 202,
                stamina: 303,
                ..Stats::default()
            },
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
        [
            attr_value(&values, "health"),
            attr_value(&values, "magicka"),
            attr_value(&values, "stamina"),
        ],
        [101.0, 202.0, 303.0],
        "malformed class data means the DNAM attribute cache is kept"
    );
}

// 9. A deleted CLAS record must be treated as unresolved, never read for its
// stale attribute weights. Mirrors `actor_skills_adversarial.rs`'s
// equivalent skill test for the new attribute path.
#[test]
fn deleted_class_record_keeps_the_dnam_attribute_cache_instead_of_stale_weights() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(clas(0x900, [5, 5, 5]).with_flags(soe_esm::plugin::flags::DELETED));
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
            Stats {
                health: 115,
                magicka: 225,
                stamina: 335,
                ..Stats::default()
            },
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
        [
            attr_value(&values, "health"),
            attr_value(&values, "magicka"),
            attr_value(&values, "stamina"),
        ],
        [115.0, 225.0, 335.0],
        "a deleted class record must not be read for its stale weights"
    );
}

// 10. Vanilla grounding, mirroring
// `prg_15_actor_skills.rs`'s equivalent survey for skills: the attribute
// formula must run cleanly over the shipped masters including all three DLC
// (in scope per this repo's working rules) and never produce a non-finite
// health, magicka or stamina, for both manual and class-formula automatic
// actors. Skipped without SKYRIM_DATA.
#[test]
fn vanilla_autocalculated_attributes_stay_finite_across_the_dlc() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping PRG-03 vanilla attribute survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let game = Game::from_shared_order(order);
    let world = game.world();
    let mut checked = 0usize;
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
        for attribute in ["health", "magicka", "stamina"] {
            let value = attr_value(&values, attribute);
            assert!(
                value.is_finite(),
                "{npc_id}.{attribute} is not finite: {value}"
            );
        }
        checked += 1;
        let autocalc = npc.config.flags & npc::flags::AUTO_CALC_STATS != 0;
        let pc_level_mult = npc.config.flags & npc::flags::PC_LEVEL_MULT != 0;
        if autocalc
            && !pc_level_mult
            && npc.class.filter(|raw| *raw != 0).is_some_and(|raw| {
                world.resolve_from(npc_id, raw).is_some_and(|class_id| {
                    world
                        .record(class_id)
                        .is_some_and(|r| !r.is_deleted() && r.tag == Tag(*b"CLAS"))
                })
            })
        {
            class_formula_eligible += 1;
        }
    }
    assert!(
        checked > 100,
        "expected many real NPCs in the vanilla attribute survey, got {checked}"
    );
    assert!(
        class_formula_eligible > 50,
        "expected the class attribute formula to be eligible for many real auto-calculated \
         NPCs, got {class_formula_eligible}"
    );
}

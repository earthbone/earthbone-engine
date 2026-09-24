// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Planned check for PRG-03: "Class and level fixtures derive health,
//! magicka and stamina from records instead of cached hand-supplied values."
//!
//! Sources: `engine-refs/uesp-modfileformat/CLAS.wikitext` ("Weight
//! Calculations" / "Attributes": racial bonus, plus NPC ACBS offset, plus
//! 10 times Level minus 1 divided by the sum of weights, times the
//! attribute weight, with health additionally getting 5 times Level minus 1
//! independent of its own weight), `engine-refs/TES5Edit/Core/
//! wbDefinitionsTES5.pas` (CLAS and RACE DATA layouts) and `skymp
//! GetBaseActorValues.cpp` (manual actors use race start plus ACBS offset,
//! the branch this auto-calculated formula sits beside). Fixtures mirror
//! `crates/game/tests/actor_values.rs`'s `race()` helper and
//! `crates/game/tests/actor_attributes_adversarial.rs`'s `clas()` helper.

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
/// magicka and stamina as the only fields these tests vary.
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

// The done check itself: a class fixture (weights [2, 1, 1]) and a level
// fixture (5) together must derive health, magicka and stamina from the
// RACE start, the CLAS weights and the NPC's own level and ACBS offsets,
// never from the NPC's cached DNAM Stats block. That block is stuffed with
// an implausible decoy (9999) so any code path that still reads it produces
// an unmistakably wrong number.
// total_weight = 2+1+1 = 4, level 5: points = 10*4 = 40, per_set = 10,
// remainder = 0. health = 100 + 5*4 + 10*2 + 20(offset) = 160.
// magicka = 50 + 10*1 - 10(offset) = 50. stamina = 60 + 10*1 + 15(offset) = 85.
// A level 1 fixture on the same class and race would instead read
// health = 100 + 0 + 0 + 20 = 120, confirming the level fixture (not just
// the class fixture) drives the derived numbers.
#[test]
fn prg_03_autocalc_actor_derives_health_magicka_stamina_from_class_and_level_records() {
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
        "the class and level formula, not the 9999 DNAM cache, must supply health"
    );
    assert_eq!(
        attr_value(&values, "magicka"),
        50.0,
        "the class and level formula, not the 9999 DNAM cache, must supply magicka"
    );
    assert_eq!(
        attr_value(&values, "stamina"),
        85.0,
        "the class and level formula, not the 9999 DNAM cache, must supply stamina"
    );

    let mut level1 = Plugin::new(FileHeader::default());
    level1.push_top(race(0x700, 100.0, 50.0, 60.0));
    level1.push_top(clas(0x900, [2, 1, 1]));
    level1.push_top(
        npc::build_record(
            0x800,
            "FormulaLevel1",
            "FormulaLevel1",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 1,
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
    let mut level1_order = LoadOrder::new();
    level1_order.add("Actors.esm", level1).unwrap();
    let level1_game = Game::new(level1_order);
    let level1_values = level1_game
        .world()
        .initial_actor_values(id(0, 0x800))
        .unwrap();
    assert_eq!(
        attr_value(&level1_values, "health"),
        120.0,
        "the level fixture must change the derived health (100 racial + 0 level bonus \
         + 0 weighted share + 20 offset), not just the class fixture"
    );
}

// Vanilla grounding, mirroring `prg_15_actor_skills.rs`'s equivalent survey
// for skills: the class attribute formula must run cleanly over the shipped
// masters, including all three DLC, and never produce a non-finite health,
// magicka or stamina for a class-formula eligible auto-calculated actor.
// Skipped without SKYRIM_DATA.
#[test]
fn prg_03_vanilla_class_attribute_formula_stays_finite_across_the_dlc() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping prg_03 vanilla attribute survey");
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
        let autocalc = npc.config.flags & npc::flags::AUTO_CALC_STATS != 0;
        let pc_level_mult = npc.config.flags & npc::flags::PC_LEVEL_MULT != 0;
        let eligible = autocalc
            && !pc_level_mult
            && npc.class.filter(|raw| *raw != 0).is_some_and(|raw| {
                world.resolve_from(npc_id, raw).is_some_and(|class_id| {
                    world
                        .record(class_id)
                        .is_some_and(|r| !r.is_deleted() && r.tag == Tag(*b"CLAS"))
                })
            });
        if eligible {
            class_formula_eligible += 1;
            for attribute in ["health", "magicka", "stamina"] {
                let value = attr_value(&values, attribute);
                assert!(
                    value.is_finite(),
                    "{npc_id}.{attribute} is not finite: {value}"
                );
            }
        }
    }
    assert!(
        class_formula_eligible > 50,
        "expected the class attribute formula to be eligible for many real auto-calculated \
         NPCs, got {class_formula_eligible}"
    );
}

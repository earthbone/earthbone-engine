// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Independent record bytes distinguish absolute and proportional thresholds.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{flags, template},
    Game,
};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn npc(object: u32, absolute: Option<u16>, class: Option<u32>, parent: Option<u32>) -> Record {
    let mut acbs = vec![0; 24];
    if let Some(health) = absolute {
        acbs[..4].copy_from_slice(&flags::BLEEDOUT_OVERRIDE.to_le_bytes());
        acbs[22..24].copy_from_slice(&health.to_le_bytes());
    }
    if parent.is_some() {
        acbs[18..20].copy_from_slice(&template::STATS.to_le_bytes());
    }
    let mut record = Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"ACBS"), acbs);
    if let Some(class) = class {
        record = record.sub(Tag(*b"CNAM"), class.to_le_bytes());
    }
    if let Some(parent) = parent {
        record = record.sub(Tag(*b"TPLT"), parent.to_le_bytes());
    }
    record
}

fn class(fraction: f32) -> Record {
    let mut data = vec![0; 36];
    data[24..28].copy_from_slice(&fraction.to_le_bytes());
    Record::new(Tag(*b"CLAS"), 0x700).sub(Tag(*b"DATA"), data)
}

fn fixture(records: Vec<Record>, patch: Option<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Bleedout.esm", plugin).unwrap();
    if let Some(record) = patch {
        let mut patch = Plugin::new(FileHeader {
            masters: vec!["Bleedout.esm".into()],
            ..FileHeader::default()
        });
        patch.push_top(record);
        order.add("Patch.esp", patch).unwrap();
    }
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 200.)
        .unwrap();
    game
}

#[test]
fn bleedout_class_uses_permanent_health_and_winning_override() {
    let game = fixture(
        vec![npc(0x800, None, Some(0x700), None), class(0.1)],
        Some(class(0.25)),
    );
    let mut world = game.world_mut();
    world.mod_actor_value(id(0x900), "health", 40.).unwrap();
    world
        .actor(id(0x900))
        .unwrap()
        .values
        .modify_temporary("health", 100.)
        .unwrap();
    world
        .actor(id(0x900))
        .unwrap()
        .values
        .damage("health", 200.)
        .unwrap();
    assert_eq!(world.actor_bleedout_threshold(id(0x900)).unwrap(), 60.);
}

#[test]
fn bleedout_absolute_override_comes_from_selected_stats_template() {
    let game = fixture(
        vec![
            npc(0x800, Some(90), Some(0xBAD), Some(0x801)),
            npc(0x801, Some(37), Some(0xBAD), None),
        ],
        None,
    );
    // An absolute override returns before a missing class is read or health scaled.
    assert_eq!(
        game.world_mut()
            .actor_bleedout_threshold(id(0x900))
            .unwrap(),
        37.
    );
}

#[test]
fn bleedout_absent_class_reads_live_setting_and_engine_default() {
    let game = fixture(vec![npc(0x800, None, None, None)], None);
    let mut world = game.world_mut();
    assert_eq!(world.actor_bleedout_threshold(id(0x900)).unwrap(), 40.);
    world
        .settings
        .insert("fbleedoutdefault".into(), Value::Float(0.15));
    assert!((world.actor_bleedout_threshold(id(0x900)).unwrap() - 30.).abs() < 0.00001);
    world
        .settings
        .insert("fbleedoutdefault".into(), Value::Float(f32::NAN));
    assert!(world.actor_bleedout_threshold(id(0x900)).is_err());
}

#[test]
fn bleedout_threshold_rejects_bad_class_data_and_signed_override() {
    for record in [
        class(f32::NAN),
        class(-0.1),
        Record::new(Tag(*b"CLAS"), 0x700).sub(Tag(*b"DATA"), vec![0; 35]),
        Record::new(Tag(*b"WEAP"), 0x700),
    ] {
        let game = fixture(vec![npc(0x800, None, Some(0x700), None), record], None);
        assert!(game
            .world_mut()
            .actor_bleedout_threshold(id(0x900))
            .is_err());
    }
    let game = fixture(vec![npc(0x800, Some(u16::MAX), None, None)], None);
    assert!(game
        .world_mut()
        .actor_bleedout_threshold(id(0x900))
        .is_err());
}

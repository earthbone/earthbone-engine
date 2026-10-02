// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-33: `World::actor_controller_scale` supplies the linear scale a
//! runtime-generated actor controller capsule needs when a skeleton carries
//! no authored `CHAR_CONTROLLER` phantom. The formula under test is
//! `race.height[sex] * npc.height`, `TESNPC::GetHeight()` in
//! CommonLibSSE-NG's `src/RE/T/TESNPC.cpp`, both read from the NPC that
//! supplies the Traits template group.
//!
//! `crates/physics/tests/cmb_33_generated_controller.rs` proves the capsule
//! this scale feeds. `crates/godot-host/tests/cmb_33_actor_controller_geometry.rs`
//! ties the two together end to end, including against installed skeletons.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, template, Config, Stats},
    World,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC_FORM: u32 = 0x100;
const RACE_FORM: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

/// A minimal `RACE` `DATA` prefix with only the height fields set to
/// something other than the 1.0 default, so a wrong offset would be caught.
fn race(raw: u32, height_male: f32, height_female: f32) -> Record {
    let mut bytes = vec![0u8; 124];
    bytes[16..20].copy_from_slice(&height_male.to_le_bytes());
    bytes[20..24].copy_from_slice(&height_female.to_le_bytes());
    bytes[24..28].copy_from_slice(&1.0f32.to_le_bytes());
    bytes[28..32].copy_from_slice(&1.0f32.to_le_bytes());
    Record::new(T(b"RACE"), raw).sub(T(b"DATA"), bytes)
}

fn npc(form: u32, flags: u32, race: u32, height: f32) -> Record {
    let rec = npc::build_record(
        form,
        "Actor",
        "Actor",
        Config {
            flags,
            template_flags: template::TRAITS,
            level: 1,
            ..Config::default()
        },
        Stats::default(),
    )
    .sub(T(b"RNAM"), race.to_le_bytes());
    npc::with_height_weight(rec, height, 1.0)
}

fn world(records: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Scale.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn cmb_33_actor_controller_scale_multiplies_race_height_by_npc_height() {
    let w = world(vec![
        race(RACE_FORM, 1.05, 0.98),
        npc(NPC_FORM, 0, RACE_FORM, 1.2),
    ]);
    let scale = w.actor_controller_scale(id(NPC_FORM)).unwrap();
    assert!(
        (scale - 1.05 * 1.2).abs() < 0.0001,
        "expected race height_male * npc height, got {scale}"
    );
}

#[test]
fn cmb_33_actor_controller_scale_selects_the_npc_sex() {
    let w = world(vec![
        race(RACE_FORM, 1.05, 0.90),
        npc(NPC_FORM, npc::flags::FEMALE, RACE_FORM, 1.0),
    ]);
    let scale = w.actor_controller_scale(id(NPC_FORM)).unwrap();
    assert!(
        (scale - 0.90).abs() < 0.0001,
        "a female NPC must use race height_female, got {scale}"
    );
}

#[test]
fn cmb_33_actor_controller_scale_defaults_to_one_without_a_race() {
    let w = world(vec![npc(NPC_FORM, 0, 0, 1.3)]);
    let scale = w.actor_controller_scale(id(NPC_FORM)).unwrap();
    assert!(
        (scale - 1.3).abs() < 0.0001,
        "with no resolvable race, race height must fall back to 1.0, got {scale}"
    );
}

#[test]
fn cmb_33_actor_controller_scale_follows_the_traits_template_source() {
    // The base actor supplies no Traits group of its own, so the race and
    // height must come from the template it points TPLT/RNAM-free at, the
    // same source `actor_base_race` reads.
    const TEMPLATE: u32 = 0x300;
    let base = npc::build_record(
        NPC_FORM,
        "Actor",
        "Actor",
        Config {
            template_flags: template::TRAITS,
            level: 1,
            ..Config::default()
        },
        Stats::default(),
    )
    .sub(T(b"TPLT"), TEMPLATE.to_le_bytes());
    let template = npc(TEMPLATE, 0, RACE_FORM, 1.5);
    let w = world(vec![base, template, race(RACE_FORM, 1.1, 1.0)]);
    let scale = w.actor_controller_scale(id(NPC_FORM)).unwrap();
    assert!(
        (scale - 1.1 * 1.5).abs() < 0.0001,
        "expected the Traits template's race and height, got {scale}"
    );
    assert_eq!(
        w.actor_base_race(id(NPC_FORM)).unwrap(),
        Some(id(RACE_FORM)),
        "actor_controller_scale must read the same race actor_base_race does"
    );
}

#[test]
fn cmb_33_actor_controller_scale_rejects_a_nonpositive_result() {
    let w = world(vec![
        race(RACE_FORM, 0.0, 1.0),
        npc(NPC_FORM, 0, RACE_FORM, 1.0),
    ]);
    assert!(
        w.actor_controller_scale(id(NPC_FORM)).is_err(),
        "a zero race height must not silently produce a zero-scale capsule"
    );
}

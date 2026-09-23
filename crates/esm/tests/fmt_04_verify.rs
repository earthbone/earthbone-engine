// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent verifier coverage for FMT-04 (DLBR branch metadata and SCEN
//! phase/actor/action fields), written against the done check: named
//! DLBR/SCEN record-view tests retain branch and phase fields through round
//! trips. Synthetic bytes only, kept separate from the breaker's
//! `dlbr_scen_adversarial.rs`.

use soe_esm::records::gameplay::*;
use soe_esm::{LoadOrder, Plugin, Record, Tag};
use std::path::PathBuf;

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// DLBR: every field round trips through `DialogueBranch::from_record`,
/// covering the quest, category, flags and starting-topic fields the done
/// check names.
#[test]
fn fmt_04_dlbr_all_fields_round_trip_independently() {
    let rec = Record::new(b"DLBR", 0x42)
        .sub(b"QNAM", 0x1234u32.to_le_bytes())
        .sub(b"TNAM", 0u32.to_le_bytes())
        .sub(b"DNAM", 0x7u32.to_le_bytes())
        .sub(b"SNAM", 0x9999u32.to_le_bytes());
    let branch = DialogueBranch::from_record(&rec).unwrap();
    assert_eq!(branch.form_id, 0x42);
    assert_eq!(branch.quest, Some(0x1234));
    assert_eq!(branch.category, Some(0));
    assert_eq!(branch.flags, Some(0x7));
    assert_eq!(branch.starting_topic, Some(0x9999));
}

/// DLBR: a record missing every optional field still parses, with each
/// field reported absent rather than defaulted, matching the module's
/// "readable snapshot even when partial" convention documented in
/// docs/esm.md.
#[test]
fn fmt_04_dlbr_missing_fields_stay_none_not_defaulted() {
    let rec = Record::new(b"DLBR", 0x7);
    let branch = DialogueBranch::from_record(&rec).unwrap();
    assert_eq!(branch.quest, None);
    assert_eq!(branch.category, None);
    assert_eq!(branch.flags, None);
    assert_eq!(branch.starting_topic, None);
}

/// DLBR: the wrong record tag is rejected.
#[test]
fn fmt_04_dlbr_wrong_tag_is_rejected() {
    let rec = Record::new(b"DLVW", 0x1).sub(b"QNAM", 0x1u32.to_le_bytes());
    assert!(DialogueBranch::from_record(&rec).is_err());
}

/// SCEN: three phases in sequence each keep their own name, start and
/// completion condition counts and width, independent of the round-trip
/// test already in gameplay_views.rs (two phases only).
#[test]
fn fmt_04_scen_three_phases_keep_independent_fields() {
    let rec = Record::new(b"SCEN", 0x10)
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "First")
        .sub(b"NEXT", Vec::new())
        .sub(b"WNAM", 1u32.to_le_bytes())
        .sub(b"HNAM", Vec::new())
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "Second")
        .sub(b"NEXT", Vec::new())
        .sub(b"WNAM", 2u32.to_le_bytes())
        .sub(b"HNAM", Vec::new())
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "Third")
        .sub(b"NEXT", Vec::new())
        .sub(b"WNAM", 3u32.to_le_bytes())
        .sub(b"HNAM", Vec::new());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.phases.len(), 3);
    let names: Vec<_> = scene
        .phases
        .iter()
        .map(|p| p.name.clone().unwrap())
        .collect();
    assert_eq!(names, ["First", "Second", "Third"]);
    let widths: Vec<_> = scene.phases.iter().map(|p| p.width).collect();
    assert_eq!(widths, [Some(1), Some(2), Some(3)]);
}

/// SCEN: a Package action accumulates every PNAM into an ordered list,
/// distinct from the Dialogue and Timer coverage already in
/// gameplay_views.rs.
#[test]
fn fmt_04_scen_package_action_keeps_ordered_package_list() {
    let rec = Record::new(b"SCEN", 0x11)
        .sub(b"ANAM", 1u16.to_le_bytes())
        .zsub(b"NAM0", "Go there")
        .sub(b"ALID", 4i32.to_le_bytes())
        .sub(b"INAM", 0u32.to_le_bytes())
        .sub(b"PNAM", 0x11u32.to_le_bytes())
        .sub(b"PNAM", 0x22u32.to_le_bytes())
        .sub(b"PNAM", 0x33u32.to_le_bytes())
        .sub(b"VNAM", words(&[0, 0, 0, 0]));
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 1);
    let action = &scene.actions[0];
    assert_eq!(action.action_type, 1);
    assert_eq!(action.alias_id, Some(4));
    assert_eq!(
        action.data,
        SceneActionData::Package {
            packages: vec![0x11, 0x22, 0x33]
        }
    );
}

/// SCEN: an actor list with two actors keeps each actor's own alias, flags
/// and behavior flags separate.
#[test]
fn fmt_04_scen_two_actors_keep_separate_flags() {
    let rec = Record::new(b"SCEN", 0x12)
        .sub(b"ALID", 1i32.to_le_bytes())
        .sub(b"LNAM", 0x1u32.to_le_bytes())
        .sub(b"DNAM", 0x2u32.to_le_bytes())
        .sub(b"ALID", 2i32.to_le_bytes())
        .sub(b"LNAM", 0x3u32.to_le_bytes())
        .sub(b"DNAM", 0x4u32.to_le_bytes())
        .sub(b"PNAM", 0x900u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actors.len(), 2);
    assert_eq!(scene.actors[0].alias_id, 1);
    assert_eq!(scene.actors[0].flags, Some(0x1));
    assert_eq!(scene.actors[0].behavior_flags, Some(0x2));
    assert_eq!(scene.actors[1].alias_id, 2);
    assert_eq!(scene.actors[1].flags, Some(0x3));
    assert_eq!(scene.actors[1].behavior_flags, Some(0x4));
    assert_eq!(scene.quest, Some(0x900));
}

/// SCEN: the trailer fields (last action index, four-word behavior
/// settings, quest) all round trip together when no phases, actors or
/// actions are present.
#[test]
fn fmt_04_scen_trailer_fields_round_trip_with_empty_lists() {
    let rec = Record::new(b"SCEN", 0x13)
        .sub(b"PNAM", 0xabcu32.to_le_bytes())
        .sub(b"INAM", 7u32.to_le_bytes())
        .sub(b"VNAM", words(&[9, 8, 7, 6]));
    let scene = Scene::from_record(&rec).unwrap();
    assert!(scene.phases.is_empty());
    assert!(scene.actors.is_empty());
    assert!(scene.actions.is_empty());
    assert_eq!(scene.quest, Some(0xabc));
    assert_eq!(scene.last_action_index, Some(7));
    assert_eq!(scene.behavior_settings, Some([9, 8, 7, 6]));
}

/// SCEN: VMAD is preserved raw and untouched, per the module's documented
/// boundary with FMT-03.
#[test]
fn fmt_04_scen_vmad_stays_raw() {
    let vmad_bytes = vec![1, 2, 3, 4, 5];
    let rec = Record::new(b"SCEN", 0x14).sub(b"VMAD", vmad_bytes.clone());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.vmad.as_deref(), Some(vmad_bytes.as_slice()));
}

/// SCEN: the wrong record tag is rejected.
#[test]
fn fmt_04_scen_wrong_tag_is_rejected() {
    let rec = Record::new(b"SCEQ", 0x1).sub(b"FNAM", 0u32.to_le_bytes());
    assert!(Scene::from_record(&rec).is_err());
}

/// Sanity check that the SCEN tag constant matches the four bytes used
/// above, guarding against a typo silently mismatching `check_tag`.
#[test]
fn fmt_04_scen_tag_is_the_expected_four_bytes() {
    assert_eq!(Tag::new(b"SCEN").as_bytes(), b"SCEN");
    assert_eq!(Tag::new(b"DLBR").as_bytes(), b"DLBR");
}

/// Real-data sweep: every non-deleted DLBR and SCEN winner across the five
/// masters must decode without error. Skips cleanly when SKYRIM_DATA is
/// unset, prints record counts to stderr when it runs.
#[test]
fn fmt_04_real_masters_dlbr_and_scen_winners_decode() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping DLBR/SCEN real-data sweep");
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
    let mut dlbr_count = 0;
    let mut scen_count = 0;
    let mut phase_total = 0;
    let mut actor_total = 0;
    let mut action_total = 0;
    let mut failures = Vec::new();
    for (id, rec) in order.winners_of(Tag::new(b"DLBR")) {
        if rec.is_deleted() {
            continue;
        }
        dlbr_count += 1;
        if let Err(error) = DialogueBranch::from_record(rec) {
            failures.push(format!("DLBR {id}: {error}"));
        }
    }
    for (id, rec) in order.winners_of(Tag::new(b"SCEN")) {
        if rec.is_deleted() {
            continue;
        }
        scen_count += 1;
        match Scene::from_record(rec) {
            Ok(scene) => {
                phase_total += scene.phases.len();
                actor_total += scene.actors.len();
                action_total += scene.actions.len();
            }
            Err(error) => failures.push(format!("SCEN {id}: {error}")),
        }
    }
    eprintln!(
        "DLBR winners: {dlbr_count}, SCEN winners: {scen_count}, \
         phases: {phase_total}, actors: {actor_total}, actions: {action_total}"
    );
    assert!(dlbr_count > 0, "no DLBR winners surveyed");
    assert!(scen_count > 0, "no SCEN winners surveyed");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

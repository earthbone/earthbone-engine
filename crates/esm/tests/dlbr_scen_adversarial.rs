// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for FMT-04 (DLBR and SCEN) that the round trip tests
//! in gameplay_views.rs do not reach: a repurposed field, truncated and
//! malformed input, and a duplicate identifier pattern documented as
//! occurring in real Skyrim.esm scenes. Read-only against
//! DialogueBranch::from_record and Scene::from_record, no source under test
//! is changed here.
//!
//! Sources: uesp-modfileformat/SCEN.wikitext (Phases, Actions, and the noted
//! SNAM reuse in Timer actions) and TES5Edit/Core/wbDefinitionsTES5.pas
//! (DLBR 7114 to 7130, SCEN 7232 to 7428).

use soe_esm::records::gameplay::*;
use soe_esm::Record;

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn condition(flags: u8, comparison: u32, function: u16, param: u32) -> Vec<u8> {
    let mut bytes = vec![flags, 0xaa, 0xbb, 0xcc];
    bytes.extend(comparison.to_le_bytes());
    bytes.extend(function.to_le_bytes());
    bytes.extend([0xdd, 0xee]);
    bytes.extend(words(&[param, 7, 2, 0x800, u32::MAX]));
    bytes
}

/// uesp-modfileformat/SCEN.wikitext line 153: "SNAM is repurposed within
/// timer actions and appears in addition to the action start phase SNAM
/// field." A Timer action (type 2) therefore carries two SNAM subrecords in
/// file order, the plain u32 Start Phase from the shared action fields
/// first, then the f32 Duration from the Timer union second. The reader
/// routes every SNAM in a Timer action to the union's duration field
/// whenever action_type == 2, so the first SNAM never reaches start_phase.
#[test]
fn scene_timer_action_start_phase_is_lost_to_repurposed_snam() {
    let rec = Record::new(b"SCEN", 0x10)
        .sub(b"ANAM", 2u16.to_le_bytes())
        .sub(b"ALID", (-1i32).to_le_bytes())
        .sub(b"INAM", 7u32.to_le_bytes())
        .sub(b"SNAM", 3u32.to_le_bytes()) // Start Phase, shared field
        .sub(b"ENAM", 5u32.to_le_bytes())
        .sub(b"SNAM", 2.5f32.to_le_bytes()) // Duration, Timer union field
        .sub(b"ANAM", Vec::new())
        .sub(b"PNAM", 0x999u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 1);
    let action = &scene.actions[0];
    assert_eq!(action.action_type, 2);
    assert_eq!(action.end_phase, Some(5));
    assert_eq!(
        action.data,
        SceneActionData::Timer {
            duration: Some(2.5)
        }
    );
    assert_eq!(
        action.start_phase,
        Some(3),
        "Timer action Start Phase (first SNAM) must survive alongside Duration (second SNAM)"
    );
}

/// Phases carry no count (uesp-modfileformat/SCEN.wikitext line 5), so a
/// file truncated mid-phase, with a Start Marker HNAM and no End Marker
/// HNAM, must still surface the partial phase rather than drop it silently.
#[test]
fn scene_flushes_an_unterminated_final_phase_at_record_end() {
    let rec = Record::new(b"SCEN", 1)
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "orphan")
        .sub(b"WNAM", 77u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.phases.len(), 1);
    assert_eq!(scene.phases[0].name.as_deref(), Some("orphan"));
    assert_eq!(scene.phases[0].width, Some(77));
}

/// The same flush applies to an actor list truncated with no trailing PNAM
/// or ANAM to close it, and to an action list truncated with no closing
/// empty ANAM marker: each list keeps its own `current_*` state, so each
/// needs its own end of record flush.
#[test]
fn scene_flushes_unterminated_actor_and_action_lists_at_record_end() {
    let actor_only = Record::new(b"SCEN", 2)
        .sub(b"ALID", 9i32.to_le_bytes())
        .sub(b"LNAM", 0x11u32.to_le_bytes());
    let scene = Scene::from_record(&actor_only).unwrap();
    assert_eq!(scene.actors.len(), 1);
    assert_eq!(scene.actors[0].alias_id, 9);
    assert_eq!(scene.actors[0].flags, Some(0x11));

    let action_only = Record::new(b"SCEN", 3)
        .sub(b"ANAM", 1u16.to_le_bytes())
        .sub(b"INAM", 4u32.to_le_bytes());
    let scene = Scene::from_record(&action_only).unwrap();
    assert_eq!(scene.actions.len(), 1);
    assert_eq!(scene.actions[0].action_type, 1);
    assert_eq!(scene.actions[0].index, Some(4));
}

/// uesp-modfileformat/SCEN.wikitext line 112 documents two real Skyrim.esm
/// scenes (T03WorshippersScene, DGScene04) whose actions carry overlapping
/// INAM numbers. Actions are read into a plain Vec keyed by file order, not
/// by INAM, so a duplicate index must not merge or drop an entry.
#[test]
fn scene_retains_duplicate_action_indices_like_real_skyrim_scenes() {
    let rec = Record::new(b"SCEN", 4)
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 5u32.to_le_bytes())
        .sub(b"ANAM", Vec::new())
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 5u32.to_le_bytes())
        .sub(b"ANAM", Vec::new())
        .sub(b"PNAM", 0x123u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 2);
    assert_eq!(scene.actions[0].index, Some(5));
    assert_eq!(scene.actions[1].index, Some(5));
    assert_eq!(scene.quest, Some(0x123));
}

/// A CTDA inside a phase's condition lists goes through the same 32-byte
/// Fields::exact check as every other CTDA reader in this crate. A phase
/// with a truncated CTDA must return an error, not silently accept the
/// short data or panic while indexing it.
#[test]
fn scene_malformed_condition_inside_phase_returns_error_not_panic() {
    let rec = Record::new(b"SCEN", 5)
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "x")
        .sub(b"CTDA", vec![0u8; 10]);
    assert!(Scene::from_record(&rec).is_err());

    let rec = Record::new(b"SCEN", 6)
        .sub(b"HNAM", Vec::new())
        .zsub(b"NAM0", "x")
        .sub(b"CTDA", condition(0, 1f32.to_bits(), 1, 0))
        .sub(b"NEXT", Vec::new())
        .sub(b"CTDA", vec![0u8; 31]);
    assert!(Scene::from_record(&rec).is_err());
}

/// DialogueBranch's four fields are all plain optional FormIDs read through
/// the same fixed 4-byte path as every other form_id field in this crate.
/// Wrong-size subrecords and the wrong record tag must return errors, never
/// panic on a short slice.
#[test]
fn dlbr_malformed_field_sizes_return_errors_not_panics() {
    for tag in [*b"QNAM", *b"TNAM", *b"DNAM", *b"SNAM"] {
        for size in 0..4 {
            let rec = Record::new(b"DLBR", 7).sub(tag, vec![0u8; size]);
            assert!(
                DialogueBranch::from_record(&rec).is_err(),
                "{tag:?} at size {size} should be rejected"
            );
        }
    }
    let rec = Record::new(b"DLBR", 7)
        .sub(b"QNAM", vec![0u8; 5])
        .sub(b"SNAM", 1u32.to_le_bytes());
    assert!(DialogueBranch::from_record(&rec).is_err());
    let wrong_tag = Record::new(b"WRLD", 7);
    assert!(DialogueBranch::from_record(&wrong_tag).is_err());
}

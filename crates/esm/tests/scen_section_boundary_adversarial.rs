// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for Scene::from_record's Actions-to-Trailer boundary.
//! PNAM and VNAM each carry two unrelated meanings depending on section
//! (per-action Package entry versus record trailer field), and the reader
//! picks which meaning applies from `current_action`'s presence rather than
//! from an explicit terminator. These tests probe what happens when a real
//! or malformed file leaves an action open across that boundary.
//!
//! Source: TES5Edit/Core/wbDefinitionsTES5.pas SCEN 7232 to 7397 (Actions
//! array, then trailing PNAM/INAM/VNAM/CTDA), uesp-modfileformat/SCEN.wikitext.

use soe_esm::records::gameplay::*;
use soe_esm::Record;

/// A stray PNAM inside an open Dialogue action (type 0) must leave the
/// action's DATA topic alone. TES5Edit's wbRUnion scopes PNAM to Package
/// actions (type 1) the way it scopes DATA to Dialogue actions. The first
/// reader had no type check on PNAM and replaced the topic with a Package
/// variant. This test found that defect and now guards the fix.
#[test]
fn scen_stray_pnam_leaves_dialogue_action_data_alone() {
    let rec = Record::new(b"SCEN", 10)
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 1u32.to_le_bytes())
        .sub(b"DATA", 0x700u32.to_le_bytes())
        .sub(b"PNAM", 0x900u32.to_le_bytes())
        .sub(b"ANAM", Vec::new())
        .sub(b"PNAM", 0x999u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 1);
    let action = &scene.actions[0];
    assert_eq!(action.action_type, 0, "action_type is still Dialogue");
    assert_eq!(
        action.data,
        SceneActionData::Dialogue { topic: Some(0x700) },
        "a stray PNAM must not replace a Dialogue action's DATA topic"
    );
}

/// An action list with no closing empty ANAM leaves `current_action` set
/// when the trailer's PNAM (parent quest) arrives. Because the PNAM handler
/// treats "current_action is Some" as "this PNAM is Package data", the
/// trailing quest FormID is swallowed into the open action's package list
/// and `scene.quest` stays `None`, silently losing the quest link rather
/// than surfacing a missing-terminator error.
#[test]
fn scen_unterminated_action_list_swallows_trailer_quest_into_package_data() {
    let rec = Record::new(b"SCEN", 11)
        .sub(b"ANAM", 1u16.to_le_bytes()) // Package action, no closing empty ANAM
        .sub(b"INAM", 2u32.to_le_bytes())
        .sub(b"PNAM", 0x555u32.to_le_bytes()); // intended as trailer quest
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 1);
    assert_eq!(
        scene.actions[0].data,
        SceneActionData::Package {
            packages: vec![0x555]
        },
        "the PNAM meant for the trailer quest field was absorbed as a \
         Package package entry of the still-open action"
    );
    assert_eq!(
        scene.quest, None,
        "current reader: without a closing empty ANAM before the trailer, \
         the quest FormID never reaches scene.quest at all"
    );
}

/// VNAM is guarded by `current_action.is_none()`, the opposite asymmetry
/// from PNAM: when an action is still open, VNAM does not get misrouted
/// into the action, it is dropped entirely by the Actions section's
/// catch-all and `behavior_settings` stays unset, again with no error.
#[test]
fn scen_unterminated_action_list_drops_trailer_vnam_silently() {
    let rec = Record::new(b"SCEN", 12)
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 1u32.to_le_bytes())
        .sub(
            b"VNAM",
            [0xAAu32, 0xBB, 0xCC, 0xDD]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(scene.actions.len(), 1);
    assert_eq!(
        scene.behavior_settings, None,
        "current reader: VNAM arriving while an action is still open is \
         silently ignored by the Actions section's catch-all arm, it neither \
         updates the open action nor sets scene.behavior_settings"
    );
}

/// The same open-action state also blocks scene-level CTDA conditions:
/// the Actions section has no CTDA arm at all, so a scene-level condition
/// that arrives before a closing empty ANAM vanishes with no error, while
/// an identical CTDA after a proper terminator is kept. This asymmetry
/// means whether a trailing condition survives depends on file layout that
/// carries no explicit marker of its own.
#[test]
fn scen_trailing_ctda_depends_silently_on_action_list_being_closed() {
    let condition_bytes = |function: u16| -> Vec<u8> {
        let mut bytes = vec![0u8, 0xaa, 0xbb, 0xcc];
        bytes.extend(1f32.to_bits().to_le_bytes());
        bytes.extend(function.to_le_bytes());
        bytes.extend([0xdd, 0xee]);
        bytes.extend(
            [0u32, 7, 2, 0x800, u32::MAX]
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        bytes
    };

    let unterminated = Record::new(b"SCEN", 13)
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 1u32.to_le_bytes())
        .sub(b"CTDA", condition_bytes(42));
    let scene = Scene::from_record(&unterminated).unwrap();
    assert!(
        scene.conditions.is_empty(),
        "current reader: a CTDA while an action is still open is silently \
         dropped, the Actions section has no CTDA arm"
    );

    let terminated = Record::new(b"SCEN", 14)
        .sub(b"ANAM", 0u16.to_le_bytes())
        .sub(b"INAM", 1u32.to_le_bytes())
        .sub(b"ANAM", Vec::new())
        .sub(b"PNAM", 0x1u32.to_le_bytes())
        .sub(b"CTDA", condition_bytes(42));
    let scene = Scene::from_record(&terminated).unwrap();
    assert_eq!(
        scene.conditions.len(),
        1,
        "the identical CTDA is kept once the action list is properly closed \
         first, confirming the earlier drop is a section-boundary effect, \
         not a general CTDA parsing failure"
    );
}

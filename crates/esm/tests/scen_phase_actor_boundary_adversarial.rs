// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for Scene::from_record's Phases-to-Actors boundary,
//! the same section-boundary defect family as
//! scen_section_boundary_adversarial.rs but one step earlier in the record:
//! an unclosed phase (an odd HNAM with no matching close) leaves
//! `current_phase` set when ALID or ANAM arrives. Those fields belong only to
//! the next section, so they close the open phase and start their own entry.
//! The first reader gated both transitions on `current_phase.is_none()` and
//! dropped the fields. These tests found that defect and now guard the fix.
//!
//! Source: TES5Edit/Core/wbDefinitionsTES5.pas SCEN 7232 to 7280 (Phases,
//! paired HNAM start and end markers, then Actors), uesp-modfileformat
//! SCEN.wikitext line 5 (Phases carry no count).

use soe_esm::records::gameplay::*;
use soe_esm::Record;

/// A phase opened by HNAM but never closed is flushed when ALID, the first
/// Actors field, arrives, and the actor with its LNAM is kept.
#[test]
fn scene_unclosed_phase_closes_before_the_actor_that_follows() {
    let rec = Record::new(b"SCEN", 20)
        .sub(b"HNAM", Vec::new()) // opens a phase, never closed by a second HNAM
        .sub(b"ALID", 9i32.to_le_bytes())
        .sub(b"LNAM", 0x11u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(
        scene.phases.len(),
        1,
        "the unterminated phase is still flushed"
    );
    assert_eq!(
        scene.actors.len(),
        1,
        "ALID behind an unclosed phase must start an actor entry"
    );
}

/// The same boundary rule for ANAM: an unclosed phase followed directly by
/// an Actions-opening ANAM is flushed and the action is kept.
#[test]
fn scene_unclosed_phase_closes_before_the_action_that_follows() {
    let rec = Record::new(b"SCEN", 21)
        .sub(b"HNAM", Vec::new()) // opens a phase, never closed by a second HNAM
        .sub(b"ANAM", 1u16.to_le_bytes())
        .sub(b"INAM", 3u32.to_le_bytes());
    let scene = Scene::from_record(&rec).unwrap();
    assert_eq!(
        scene.phases.len(),
        1,
        "the unterminated phase is still flushed"
    );
    assert_eq!(
        scene.actions.len(),
        1,
        "ANAM behind an unclosed phase must start an action entry"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::audio::AmbienceOwner;
use soe_game::ambience::Track;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn track(cell: u32, descriptor: u32, path: &str) -> Track {
    Track {
        cell: id(cell),
        descriptor: id(descriptor),
        path: path.to_owned(),
    }
}

#[test]
fn aud_09_same_cell_track_does_not_restart() {
    let selected = track(0x100, 0x200, r"Music\Field.wav");
    let mut owner = AmbienceOwner::default();

    assert!(owner.set(Some(&selected)));
    assert!(!owner.set(Some(&selected)));
}

#[test]
fn aud_09_new_cell_reselecting_same_track_changes_owner() {
    let first = track(0x100, 0x200, r"Music\Field.wav");
    let next = track(0x101, 0x200, r"Music\Field.wav");
    let mut owner = AmbienceOwner::default();

    assert!(owner.set(Some(&first)));
    assert!(owner.set(Some(&next)));
}

#[test]
fn aud_09_cell_without_eligible_track_releases_departed_owner() {
    let selected = track(0x100, 0x200, r"Music\Field.wav");
    let mut owner = AmbienceOwner::default();

    assert!(owner.set(Some(&selected)));
    assert!(owner.set(None));
    assert!(!owner.set(None));
}

#[test]
fn aud_09_new_descriptor_in_same_cell_changes_owner() {
    let first = track(0x100, 0x200, r"Music\Field.wav");
    let next = track(0x100, 0x201, r"Music\Cave.wav");
    let mut owner = AmbienceOwner::default();

    assert!(owner.set(Some(&first)));
    assert!(owner.set(Some(&next)));
}

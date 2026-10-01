// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::ambience::Track;
use soe_godot_host::audio::AmbienceOwner;

fn track(cell: u32, descriptor: u32) -> Track {
    Track {
        cell: GlobalFormId {
            file: 0,
            object: cell,
        },
        descriptor: GlobalFormId {
            file: 0,
            object: descriptor,
        },
        path: "Music/Cell.wav".to_owned(),
    }
}

#[test]
fn aud_09_departure_releases_old_owner_and_repeated_empty_transition_is_idle() {
    let mut owner = AmbienceOwner::default();
    assert!(owner.set(Some(&track(0x100, 0x200))));
    assert!(owner.set(None));
    assert_eq!(owner, AmbienceOwner::default());
    assert!(!owner.set(None));
}

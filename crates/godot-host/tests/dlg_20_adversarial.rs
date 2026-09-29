// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::Lip;
use soe_godot_host::dialogue_menu::DialogueMouth;

fn lip_track() -> soe_audio::LipWeightTrack {
    let frames = [[0.1_f32, 0.2], [0.3, 0.4], [0.5, 0.6]];
    let mut body = Vec::new();
    body.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    body.extend_from_slice(&(-1_i32).to_le_bytes());
    body.extend_from_slice(&2_u32.to_le_bytes());
    for frame in frames {
        for weight in frame {
            body.extend_from_slice(&weight.to_le_bytes());
        }
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(body.len() as u32 + 16).to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(&body);
    Lip::parse(&bytes).unwrap().decode().unwrap()
}

#[test]
fn dlg_20_replacing_a_track_clears_old_targets_and_samples_new_target_order() {
    let mut mouth = DialogueMouth::default();
    mouth.start(Some(lip_track()), 3.0);
    mouth.sample(1.0);
    assert_eq!(mouth.weights(), [0.3, 0.4]);

    mouth.start(Some(lip_track()), 3.0);
    assert_eq!(mouth.weights(), [0.0, 0.0]);
    mouth.sample(2.0);
    assert_eq!(mouth.weights(), [0.5, 0.6]);
}

#[test]
fn dlg_20_out_of_range_playback_times_return_neutral_weights() {
    let mut mouth = DialogueMouth::default();
    mouth.start(Some(lip_track()), 3.0);
    mouth.sample(1.0);
    assert_eq!(mouth.weights(), [0.3, 0.4]);

    for time in [-0.01, 3.0, f64::NAN, f64::INFINITY] {
        mouth.sample(time);
        assert_eq!(mouth.weights(), [0.0, 0.0]);
    }
}

#[test]
fn dlg_20_invalid_voice_duration_never_leaves_a_track_active() {
    for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut mouth = DialogueMouth::default();
        mouth.start(Some(lip_track()), duration);
        mouth.sample(0.5);
        assert_eq!(mouth.weights(), [0.0, 0.0]);
    }
}

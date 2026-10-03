// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::Lip;
use soe_godot_host::dialogue_menu::DialogueVoice;

fn track() -> soe_audio::LipWeightTrack {
    let frames = [[0.1_f32, 0.2], [0.3, 0.4]];
    let mut body = Vec::new();
    body.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    body.extend_from_slice(&(-3_i32).to_le_bytes());
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
fn dlg_20_seek_before_and_after_playback_range_returns_to_authored_frames() {
    let mut voice = DialogueVoice::default();
    voice.start(7, Some(track()), 1.0);

    voice.update(7, true, 0.5);
    assert_eq!(voice.weights(), [0.3, 0.4]);
    voice.update(7, true, 1.0);
    assert_eq!(voice.weights(), [0.0, 0.0]);
    voice.update(7, true, 0.0);
    assert_eq!(voice.weights(), [0.1, 0.2]);
}

#[test]
fn dlg_20_non_finite_playback_position_neutralizes_then_recovers() {
    let mut voice = DialogueVoice::default();
    voice.start(2, Some(track()), 1.0);
    voice.update(2, true, 0.0);
    voice.update(2, true, f64::NAN);
    assert_eq!(voice.weights(), [0.0, 0.0]);
    voice.update(2, true, 0.5);
    assert_eq!(voice.weights(), [0.3, 0.4]);
}

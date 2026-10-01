// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::Lip;
use soe_godot_host::dialogue_menu::DialogueVoice;

fn lip_track() -> soe_audio::LipWeightTrack {
    let frames = [[0.1_f32, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8]];
    let mut body = Vec::new();
    body.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    body.extend_from_slice(&(-2_i32).to_le_bytes());
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
fn dlg_20_breaker_breaker_end_of_voice_clears_and_late_poll_stays_neutral() {
    let mut voice = DialogueVoice::default();
    voice.start(1, Some(lip_track()), 2.0);
    voice.update(1, true, 1.5);
    assert_eq!(voice.weights(), [0.7, 0.8]);

    voice.update(1, false, 2.0);
    assert_eq!(voice.weights(), [0.0, 0.0]);
    voice.update(1, true, 1.0);
    assert_eq!(voice.weights(), [0.0, 0.0]);
}

#[test]
fn dlg_20_breaker_breaker_seek_samples_current_playback_position() {
    let mut voice = DialogueVoice::default();
    voice.start(2, Some(lip_track()), 2.0);
    voice.update(2, true, 1.5);
    assert_eq!(voice.weights(), [0.7, 0.8]);

    voice.update(2, true, 0.5);
    assert_eq!(voice.weights(), [0.3, 0.4]);
}

#[test]
fn dlg_20_breaker_breaker_stop_for_other_slot_preserves_active_track() {
    let mut voice = DialogueVoice::default();
    voice.start(3, Some(lip_track()), 2.0);
    voice.update(3, true, 1.0);
    voice.stop(4);

    assert_eq!(voice.weights(), [0.5, 0.6]);
    voice.update(4, false, 0.0);
    assert_eq!(voice.weights(), [0.5, 0.6]);
}

#[test]
fn dlg_20_breaker_breaker_restart_same_slot_uses_new_playback_position() {
    let mut voice = DialogueVoice::default();
    voice.start(5, Some(lip_track()), 2.0);
    voice.update(5, true, 1.5);
    assert_eq!(voice.weights(), [0.7, 0.8]);

    voice.start(5, Some(lip_track()), 2.0);
    assert_eq!(voice.weights(), [0.0, 0.0]);
    voice.update(5, true, 0.0);
    assert_eq!(voice.weights(), [0.1, 0.2]);
}

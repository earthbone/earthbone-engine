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
fn dlg_20_breaker_playback_time_samples_each_authored_frame_in_order() {
    let mut voice = DialogueVoice::default();
    voice.start(2, Some(lip_track()), 2.0);

    for (time, expected) in [
        (0.0, [0.1, 0.2]),
        (0.5, [0.3, 0.4]),
        (1.0, [0.5, 0.6]),
        (1.5, [0.7, 0.8]),
        (2.0, [0.0, 0.0]),
    ] {
        voice.update(2, true, time);
        assert_eq!(voice.weights(), expected);
    }
}

#[test]
fn dlg_20_breaker_old_slot_completion_cannot_clear_replacement_voice() {
    let mut voice = DialogueVoice::default();
    voice.start(2, Some(lip_track()), 2.0);
    voice.update(2, true, 0.5);
    voice.start(5, Some(lip_track()), 2.0);

    voice.update(2, false, 0.0);
    voice.update(5, true, 1.0);
    assert_eq!(voice.weights(), [0.5, 0.6]);
}

#[test]
fn dlg_20_breaker_finished_voice_clears_weights_and_ignores_late_poll() {
    let mut voice = DialogueVoice::default();
    voice.start(3, Some(lip_track()), 2.0);
    voice.update(3, true, 1.0);
    assert_eq!(voice.weights(), [0.5, 0.6]);

    voice.update(3, false, 1.0);
    assert_eq!(voice.weights(), [0.0, 0.0]);
    voice.update(3, true, 1.5);
    assert_eq!(voice.weights(), [0.0, 0.0]);
}

#[test]
fn dlg_20_breaker_voice_without_lip_resets_prior_targets_to_neutral() {
    let mut voice = DialogueVoice::default();
    voice.start(3, Some(lip_track()), 2.0);
    voice.update(3, true, 1.0);
    assert_eq!(voice.weights(), [0.5, 0.6]);

    voice.start(6, None, 2.0);
    voice.update(6, true, 0.5);
    assert!(voice.weights().iter().all(|weight| *weight == 0.0));
}

#[test]
fn dlg_20_breaker_repeated_and_stale_stop_calls_preserve_the_current_line() {
    let mut voice = DialogueVoice::default();
    voice.start(4, Some(lip_track()), 2.0);
    voice.update(4, true, 1.0);
    voice.stop(4);
    voice.stop(4);
    assert_eq!(voice.weights(), [0.0, 0.0]);

    voice.start(5, Some(lip_track()), 2.0);
    voice.update(5, true, 0.0);
    voice.stop(4);
    assert_eq!(voice.weights(), [0.1, 0.2]);
}

#[test]
fn dlg_20_breaker_invalid_duration_replacement_keeps_targets_neutral() {
    let mut voice = DialogueVoice::default();
    voice.start(1, Some(lip_track()), 2.0);
    voice.update(1, true, 0.5);
    assert_eq!(voice.weights(), [0.3, 0.4]);

    voice.start(2, Some(lip_track()), f64::NAN);
    voice.update(2, true, 0.5);
    assert_eq!(voice.weights(), [0.0, 0.0]);
}

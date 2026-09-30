#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::Lip;
use soe_godot_host::dialogue_menu::DialogueMouth;

fn lip_track() -> soe_audio::LipWeightTrack {
    let weights = [[0.1_f32, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8]];
    let mut body = Vec::new();
    body.extend_from_slice(&4_u32.to_le_bytes());
    body.extend_from_slice(&(-2_i32).to_le_bytes());
    body.extend_from_slice(&2_u32.to_le_bytes());
    for frame in weights {
        for weight in frame {
            body.extend_from_slice(&weight.to_le_bytes());
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(body.len() as u32 + 16).to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(&body);
    Lip::parse(&bytes)
        .expect("synthetic LIP header parses")
        .decode()
        .expect("synthetic native track decodes")
}

#[test]
fn dlg_20_voice_playback_samples_native_lip_frames_in_target_order() {
    let mut mouth = DialogueMouth::default();
    mouth.start(Some(lip_track()), 2.0);

    mouth.sample(0.0);
    assert_eq!(mouth.weights(), [0.1, 0.2]);
    mouth.sample(0.5);
    assert_eq!(mouth.weights(), [0.3, 0.4]);
    mouth.sample(1.0);
    assert_eq!(mouth.weights(), [0.5, 0.6]);
    mouth.sample(1.99);
    assert_eq!(mouth.weights(), [0.7, 0.8]);
}

#[test]
fn dlg_20_interrupted_voice_resets_mouth_weights_to_neutral() {
    let mut mouth = DialogueMouth::default();
    mouth.start(Some(lip_track()), 2.0);
    mouth.sample(0.5);
    assert_eq!(mouth.weights(), [0.3, 0.4]);

    mouth.clear();
    assert_eq!(mouth.weights(), [0.0, 0.0]);
    mouth.sample(1.0);
    assert_eq!(mouth.weights(), [0.0, 0.0]);

    mouth.start(None, 2.0);
    assert_eq!(mouth.weights(), [0.0, 0.0]);
}

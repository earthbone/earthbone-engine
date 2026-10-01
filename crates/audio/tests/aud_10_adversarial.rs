// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(feature = "rhubarb")]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::pcm::Pcm;
use soe_audio::rhubarb::Rhubarb;
use soe_esm::records::gameplay::Response;
use soe_esm::records::Text;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

fn fixture(output: &str) -> (String, std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("soe-aud-10-{}-{id}", std::process::id()));
    let program = base.with_extension("sh");
    let captured = base.with_extension("txt");
    let script = format!(
        "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--dialogFile\" ]; then\n    cat \"$2\" > '{}'\n    shift 2\n  else\n    shift\n  fi\ndone\ncat <<'JSON'\n{}\nJSON\n",
        captured.display(),
        output
    );
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program.to_string_lossy().into_owned(), program, captured)
}

fn response(text: Option<Text>) -> Response {
    Response {
        emotion: 0,
        emotion_value: 0,
        number: 1,
        sound: 0,
        use_emotion_animation: 0,
        text,
        speaker_idle: None,
        listener_idle: None,
    }
}

fn pcm() -> Pcm {
    Pcm {
        sample_rate: 16_000,
        channels: 1,
        samples: vec![0.0; 1600],
    }
}

#[test]
fn aud_10_plain_info_text_is_passed_verbatim_as_the_dialog_hint() {
    let text = "I'm here.\nWait for me!";
    let output =
        r#"{"metadata":{"duration":0.1},"mouthCues":[{"start":0.0,"end":0.1,"value":"B"}]}"#;
    let (program, script, captured) = fixture(output);
    Rhubarb::at(program)
        .visemes_for_response(&pcm(), &response(Some(Text::Plain(text.to_string()))))
        .unwrap();

    assert_eq!(std::fs::read_to_string(&captured).unwrap(), text);
    std::fs::remove_file(script).unwrap();
    std::fs::remove_file(captured).unwrap();
}

#[test]
fn aud_10_missing_or_localized_info_text_uses_audio_only_recognition() {
    let output =
        r#"{"metadata":{"duration":0.1},"mouthCues":[{"start":0.0,"end":0.1,"value":"B"}]}"#;
    for text in [None, Some(Text::Localized(0x1234))] {
        let (program, script, captured) = fixture(output);
        Rhubarb::at(program)
            .visemes_for_response(&pcm(), &response(text))
            .unwrap();
        assert!(!captured.exists(), "unresolved text was sent as a hint");
        std::fs::remove_file(script).unwrap();
    }
}

#[test]
fn aud_10_rejects_cues_that_do_not_cover_the_reported_track_duration() {
    let output =
        r#"{"metadata":{"duration":0.1},"mouthCues":[{"start":0.02,"end":0.08,"value":"B"}]}"#;
    let (program, script, captured) = fixture(output);
    let result = Rhubarb::at(program).visemes_for_response(&pcm(), &response(None));

    assert!(
        result.is_err(),
        "Rhubarb cues failed to cover the full recording"
    );
    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_file(captured);
}

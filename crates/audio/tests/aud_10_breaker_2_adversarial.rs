// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(feature = "rhubarb")]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::pcm::Pcm;
use soe_audio::rhubarb::Rhubarb;
use soe_esm::records::gameplay::Response;
use soe_esm::records::Text;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

fn fixture() -> (String, std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("soe-aud-10-extra-{}-{id}", std::process::id()));
    let program = base.with_extension("sh");
    let captured = base.with_extension("txt");
    let json = r#"{"metadata":{"duration":0.1},"mouthCues":[{"start":0.0,"end":0.1,"value":"B"}]}"#;
    let script = format!(
        "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--dialogFile\" ]; then\n    cat \"$2\" > '{}'\n    shift 2\n  else\n    shift\n  fi\ndone\ncat <<'JSON'\n{json}\nJSON\n",
        captured.display()
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
fn aud_10_multiline_info_hint_is_written_as_data() {
    let hint = "Wait; don't leave.\n$HOME `quoted` 你好";
    let (program, script, captured) = fixture();
    Rhubarb::at(program)
        .visemes_for_response(&pcm(), &response(Some(Text::Plain(hint.to_string()))))
        .expect("Rhubarb accepts the INFO response");

    assert_eq!(std::fs::read_to_string(&captured).unwrap(), hint);
    std::fs::remove_file(script).unwrap();
    std::fs::remove_file(captured).unwrap();
}

#[test]
fn aud_10_empty_plain_text_remains_a_present_info_hint() {
    let (program, script, captured) = fixture();
    Rhubarb::at(program)
        .visemes_for_response(&pcm(), &response(Some(Text::Plain(String::new()))))
        .expect("empty plain INFO text still yields audio recognition");

    assert!(captured.exists(), "present plain text uses dialogFile");
    assert_eq!(std::fs::read_to_string(&captured).unwrap(), "");
    std::fs::remove_file(script).unwrap();
    std::fs::remove_file(captured).unwrap();
}

#[test]
fn aud_10_cache_identity_distinguishes_unhinted_empty_and_changed_text() {
    let (program, script, _) = fixture();
    let rhubarb = Rhubarb::at(program);
    let unhinted = rhubarb.cache_settings(&pcm(), None).unwrap();
    let empty = rhubarb.cache_settings(&pcm(), Some("")).unwrap();
    let hinted = rhubarb.cache_settings(&pcm(), Some("first response")).unwrap();
    let changed = rhubarb.cache_settings(&pcm(), Some("second response")).unwrap();

    assert_ne!(unhinted, empty);
    assert_ne!(hinted, changed);
    std::fs::remove_file(script).unwrap();
}

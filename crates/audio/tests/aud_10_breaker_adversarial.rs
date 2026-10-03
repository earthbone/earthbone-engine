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
    let base = std::env::temp_dir().join(format!("soe-aud-10-breaker-{}-{id}", std::process::id()));
    let program = base.with_extension("sh");
    let captured = base.with_extension("txt");
    let json = r#"{"metadata":{"duration":0.1},"mouthCues":[{"start":0.0,"end":0.1,"value":"B"}]}"#;
    let script = format!(
        "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--dialogFile\" ]; then\n    cat \"$2\" > '{}'; shift 2\n  else\n    shift\n  fi\ndone\ncat <<'JSON'\n{json}\nJSON\n",
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
fn aud_10_utf8_info_hint_is_preserved_without_shell_interpretation() {
    let hint = "¡Espera! 你好, I'm here; $HOME `quoted`";
    let (program, script, captured) = fixture();
    Rhubarb::at(program)
        .visemes_for_response(&pcm(), &response(Some(Text::Plain(hint.to_string()))))
        .unwrap();

    assert_eq!(std::fs::read_to_string(&captured).unwrap(), hint);
    std::fs::remove_file(script).unwrap();
    std::fs::remove_file(captured).unwrap();
}

#[cfg(feature = "xwma-ffmpeg")]
#[test]
fn aud_10_installed_dialogue_corpus_meets_both_duration_bounds() {
    use soe_audio::viseme::VisemeSource;
    use soe_audio::wma::Ffmpeg;
    use soe_audio::{Fuz, Xwma};
    use soe_esm::records::gameplay::DialogueInfo;
    use soe_esm::{LoadOrder, Plugin, Tag};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const MAX_DURATION_ERROR_SECONDS: f64 = 0.05;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let plugin_path = data.join("Skyrim.esm");
    let archive_path = data.join("Skyrim - Voices_en0.bsa");
    if !plugin_path.exists() || !archive_path.exists() {
        eprintln!("skipping: Skyrim.esm or English voice archive is missing");
        return;
    }
    let plugin_bytes = std::fs::read(plugin_path).expect("Skyrim.esm reads");
    let plugin = Plugin::parse(&plugin_bytes).expect("Skyrim.esm parses");
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).expect("plugin loads");
    let archive = soe_bsa::Archive::open(&archive_path).expect("voice archive opens");
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: ffmpeg is not on the path");
        return;
    }
    let Some(program) = std::env::var_os("SOE_RHUBARB") else {
        eprintln!("skipping: SOE_RHUBARB is not set");
        return;
    };
    let rhubarb = Rhubarb::at(program.to_string_lossy().into_owned());
    if !rhubarb.available() {
        eprintln!("skipping: SOE_RHUBARB is unavailable");
        return;
    }

    let mut voice_files = BTreeMap::new();
    for name in archive.files() {
        let Some(stem) = name
            .rsplit(['\\', '/'])
            .next()
            .and_then(|file| file.strip_suffix(".fuz"))
        else {
            continue;
        };
        let mut fields = stem.rsplit('_');
        let Some(response_number) = fields.next().and_then(|field| field.parse::<u8>().ok()) else {
            continue;
        };
        let Some(info_id) = fields
            .next()
            .and_then(|field| u32::from_str_radix(field, 16).ok())
        else {
            continue;
        };
        voice_files
            .entry((info_id, response_number))
            .or_insert_with(|| name.clone());
    }

    let mut corpus = Vec::new();
    for (_, record) in order.winners_of(Tag::new(b"INFO")) {
        let Ok(info) = DialogueInfo::from_record(record, false) else {
            continue;
        };
        for response in info.responses {
            if !matches!(&response.text, Some(Text::Plain(text)) if !text.trim().is_empty()) {
                continue;
            }
            let Some(name) = voice_files.get(&(info.form_id, response.number)) else {
                continue;
            };
            corpus.push((info.form_id, response.number, name.clone(), response));
        }
    }
    corpus.sort_by_key(|row| (row.0, row.1));
    corpus.truncate(8);
    assert_eq!(
        corpus.len(),
        8,
        "fixed corpus must contain eight INFO lines"
    );

    for (_, _, name, response) in &corpus {
        let bytes = archive.read(name).expect("voice line reads");
        let fuz = Fuz::split(&bytes).expect("FUZ splits");
        let xwma = Xwma::parse(fuz.audio).expect("xWMA parses");
        let pcm = soe_audio::wma::decode_with(&decoder, &xwma).expect("voice decodes");
        let expected = pcm.duration_seconds();
        let hinted = rhubarb
            .visemes_for_response(&pcm, response)
            .unwrap_or_else(|error| panic!("{name} with INFO hint: {error}"));
        let unhinted = rhubarb
            .visemes(&pcm, None)
            .unwrap_or_else(|error| panic!("{name} without INFO hint: {error}"));
        assert!(
            (hinted.duration - expected).abs() <= MAX_DURATION_ERROR_SECONDS,
            "{name} hinted track exceeds the predeclared duration bound"
        );
        assert!(
            (unhinted.duration - expected).abs() <= MAX_DURATION_ERROR_SECONDS,
            "{name} unhinted track exceeds the predeclared duration bound"
        );
    }
}

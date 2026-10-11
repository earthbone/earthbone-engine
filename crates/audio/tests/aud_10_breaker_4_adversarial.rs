// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(feature = "rhubarb")]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::pcm::Pcm;
use soe_audio::rhubarb::Rhubarb;
use std::os::unix::fs::PermissionsExt;

#[test]
fn aud_10_rejects_stereo_pcm_with_an_incomplete_interleaved_frame() {
    let script =
        std::env::temp_dir().join(format!("soe-aud-10-invalid-pcm-{}.sh", std::process::id()));
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' '{\"metadata\":{\"duration\":0.1},\"mouthCues\":[{\"start\":0.0,\"end\":0.1,\"value\":\"B\"}]}'\n",
    )
    .expect("fake Rhubarb script writes");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("fake Rhubarb script becomes executable");
    let pcm = Pcm {
        sample_rate: 16_000,
        channels: 2,
        samples: vec![0.25, -0.25, 0.5],
    };

    let result = Rhubarb::at(script.to_string_lossy()).visemes(&pcm, None);

    let _ = std::fs::remove_file(script);
    assert!(result.is_err(), "incomplete stereo frame was accepted");
}

#[cfg(feature = "xwma-ffmpeg")]
#[test]
fn aud_10_fixed_info_corpus_keeps_hinted_and_unhinted_cues_within_audio_duration() {
    use soe_audio::viseme::VisemeSource;
    use soe_audio::wma::{Ffmpeg, decode_with};
    use soe_audio::{Fuz, Xwma};
    use soe_esm::records::Text;
    use soe_esm::records::gameplay::DialogueInfo;
    use soe_esm::{LoadOrder, Plugin, Tag};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const MAX_DURATION_ERROR_SECONDS: f64 = 0.05;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let rhubarb = std::env::var_os("SOE_RHUBARB");
    let Some(rhubarb) = rhubarb else {
        eprintln!("skipping: SOE_RHUBARB is not set");
        return;
    };
    let mut order = LoadOrder::new();
    let plugin = Plugin::parse(&std::fs::read(data.join("Skyrim.esm")).expect("Skyrim.esm reads"))
        .expect("Skyrim.esm parses");
    order.add("Skyrim.esm", plugin).expect("Skyrim.esm loads");
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Voices_en0.bsa"))
        .expect("English voice archive opens");
    let mut voices = BTreeMap::new();
    for name in archive.files() {
        let Some(stem) = name
            .rsplit(['\\', '/'])
            .next()
            .and_then(|file| file.strip_suffix(".fuz"))
        else {
            continue;
        };
        let mut parts = stem.rsplit('_');
        let Some(response) = parts.next().and_then(|value| value.parse::<u8>().ok()) else {
            continue;
        };
        let Some(info) = parts
            .next()
            .and_then(|value| u32::from_str_radix(value, 16).ok())
        else {
            continue;
        };
        voices
            .entry((info, response))
            .or_insert_with(|| name.clone());
    }
    let mut corpus = Vec::new();
    for (_, record) in order.winners_of(Tag::new(b"INFO")) {
        let Ok(info) = DialogueInfo::from_record(record, false) else {
            continue;
        };
        for response in info.responses {
            if matches!(&response.text, Some(Text::Plain(text)) if !text.trim().is_empty()) {
                if let Some(name) = voices.get(&(info.form_id, response.number)) {
                    corpus.push((info.form_id, response.number, name.clone(), response));
                }
            }
        }
    }
    corpus.sort_by_key(|line| (line.0, line.1));
    corpus.truncate(8);
    assert_eq!(corpus.len(), 8, "fixed INFO corpus contains eight lines");
    for (_, _, name, response) in corpus {
        let bytes = archive.read(&name).expect("voice line reads");
        let fuz = Fuz::split(&bytes).expect("FUZ splits");
        let xwma = Xwma::parse(fuz.audio).expect("xWMA parses");
        let pcm = decode_with(&Ffmpeg::new(), &xwma).expect("voice decodes");
        let duration = pcm.duration_seconds();
        let source = Rhubarb::at(rhubarb.to_string_lossy());
        for (mode, track) in [
            ("hinted", source.visemes_for_response(&pcm, &response)),
            ("unhinted", source.visemes(&pcm, None)),
        ] {
            let track = track.unwrap_or_else(|error| panic!("{name} {mode}: {error}"));
            let error = (track.duration - duration).abs();
            assert!(
                error <= MAX_DURATION_ERROR_SECONDS,
                "{name} {mode} duration error {error:.3}s"
            );
        }
    }
}

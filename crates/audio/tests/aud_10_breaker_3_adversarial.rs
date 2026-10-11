// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(all(feature = "rhubarb", feature = "xwma-ffmpeg"))]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

const MAX_TRACK_ERROR_SECONDS: f64 = 0.05;

fn rhubarb() -> Option<soe_audio::rhubarb::Rhubarb> {
    use soe_audio::rhubarb::Rhubarb;

    if let Some(program) = std::env::var_os("SOE_RHUBARB") {
        let source = Rhubarb::at(program.to_string_lossy().into_owned());
        if source.available() {
            return Some(source);
        }
    }
    let source = Rhubarb::new();
    source.available().then_some(source)
}

#[test]
fn aud_10_fixed_info_corpus_keeps_hinted_and_unhinted_tracks_within_bound() {
    use soe_audio::viseme::VisemeSource;
    use soe_audio::wma::Ffmpeg;
    use soe_audio::{Fuz, Xwma};
    use soe_esm::records::gameplay::DialogueInfo;
    use soe_esm::records::Text;
    use soe_esm::{LoadOrder, Plugin, Tag};
    use std::collections::BTreeMap;

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
    let plugin = Plugin::parse(&std::fs::read(plugin_path).expect("Skyrim.esm reads"))
        .expect("Skyrim.esm parses");
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).expect("plugin loads");
    let archive = soe_bsa::Archive::open(&archive_path).expect("voice archive opens");
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: ffmpeg is unavailable");
        return;
    }
    let source = rhubarb().expect("AUD-10 requires Rhubarb when installed data is present");

    let mut voices = BTreeMap::new();
    for name in archive.files() {
        let Some(stem) = name
            .rsplit(['\\', '/'])
            .next()
            .and_then(|file| file.strip_suffix(".fuz"))
        else {
            continue;
        };
        let mut fields = stem.rsplit('_');
        let Some(response) = fields.next().and_then(|part| part.parse::<u8>().ok()) else {
            continue;
        };
        let Some(info) = fields
            .next()
            .and_then(|part| u32::from_str_radix(part, 16).ok())
        else {
            continue;
        };
        voices.entry((info, response)).or_insert_with(|| name.clone());
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
            if let Some(name) = voices.get(&(info.form_id, response.number)) {
                corpus.push((info.form_id, response.number, name.clone(), response));
            }
        }
    }
    corpus.sort_by_key(|line| (line.0, line.1));
    corpus.truncate(8);
    assert_eq!(corpus.len(), 8, "fixed INFO corpus must contain eight lines");

    for (_, _, name, response) in corpus {
        let bytes = archive.read(&name).expect("voice line reads");
        let fuz = Fuz::split(&bytes).expect("FUZ splits");
        let xwma = Xwma::parse(fuz.audio).expect("xWMA parses");
        let pcm = soe_audio::wma::decode_with(&decoder, &xwma).expect("voice decodes");
        let audio_duration = pcm.duration_seconds();
        let hinted = source
            .visemes_for_response(&pcm, &response)
            .unwrap_or_else(|error| panic!("{name} with INFO text: {error}"));
        let unhinted = source
            .visemes(&pcm, None)
            .unwrap_or_else(|error| panic!("{name} without INFO text: {error}"));
        assert!(
            (hinted.duration - audio_duration).abs() <= MAX_TRACK_ERROR_SECONDS,
            "{name} hinted duration exceeds the predeclared bound"
        );
        assert!(
            (unhinted.duration - audio_duration).abs() <= MAX_TRACK_ERROR_SECONDS,
            "{name} unhinted duration exceeds the predeclared bound"
        );
    }
}

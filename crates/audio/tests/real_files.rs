// SPDX-License-Identifier: GPL-3.0-or-later

//! Gated on `SKYRIM_DATA`, these run the readers over the installed game's
//! voice archive. They skip cleanly when the variable is unset, and they
//! never copy game data anywhere.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::fuz::AudioKind;
use soe_audio::{Container, Fuz, Lip, Xwma};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn voice_archive() -> Option<soe_bsa::Archive> {
    let dir = data_dir()?;
    let path = dir.join("Skyrim - Voices_en0.bsa");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    Some(soe_bsa::Archive::open(&path).expect("the voice archive opens"))
}

#[test]
fn every_voice_file_splits_and_its_audio_half_parses() {
    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".fuz"))
        .cloned()
        .collect();
    assert!(!names.is_empty(), "the archive holds no fuz files");

    let started = std::time::Instant::now();
    let (mut with_lip, mut empty_lip, mut xwma_kind, mut wave_kind, mut other_kind) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut lip_words = std::collections::BTreeMap::<u32, usize>::new();
    let mut with_targets = 0usize;
    let (mut packets, mut seconds) = (0u64, 0f64);
    let mut rates = std::collections::BTreeMap::<u32, usize>::new();
    let mut channels = std::collections::BTreeMap::<u16, usize>::new();
    let mut tags = std::collections::BTreeMap::<u16, usize>::new();

    for name in &names {
        let bytes = archive.read(name).expect("a voice file reads");
        let fuz = Fuz::split(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(fuz.version, 1, "{name} has an unexpected FUZ version");

        if fuz.has_lip() {
            with_lip += 1;
            let lip = Lip::parse(fuz.lip).unwrap_or_else(|e| panic!("{name} lip: {e}"));
            *lip_words.entry(lip.flags).or_default() += 1;
            assert!(
                lip.compressed() && lip.variable_targets(),
                "{name}: every shipped block is packed with a stored target count"
            );
            let want = if lip.has_gestures() {
                soe_audio::lip::TARGETS
            } else {
                soe_audio::lip::TARGETS_NO_GESTURES
            };
            assert_eq!(lip.targets(), Some(want), "{name}: decoded target metadata");
            with_targets += 1;
        } else {
            empty_lip += 1;
        }

        match fuz.audio_kind() {
            AudioKind::Xwma => xwma_kind += 1,
            AudioKind::Wave => wave_kind += 1,
            AudioKind::NxOpus | AudioKind::OtherRiff(_) | AudioKind::Unknown => other_kind += 1,
        }

        if fuz.audio_kind() == AudioKind::Xwma {
            let x = Xwma::parse(fuz.audio).unwrap_or_else(|e| panic!("{name} xwma: {e}"));
            packets += x.packets() as u64;
            seconds += x.duration_seconds();
            *rates.entry(x.format.sample_rate).or_default() += 1;
            *channels.entry(x.format.channels).or_default() += 1;
            *tags.entry(x.format.format_tag).or_default() += 1;
            assert!(
                x.duration_seconds() > 0.0,
                "{name} has no duration, packet table missing"
            );
        }
    }

    let elapsed = started.elapsed();
    eprintln!("voice files: {}", names.len());
    eprintln!("  with lip {with_lip}, empty lip {empty_lip}");
    eprintln!("  lip flag words {lip_words:?}");
    eprintln!("  target count read {with_targets}");
    eprintln!("  audio XWMA {xwma_kind}, WAVE {wave_kind}, other {other_kind}");
    eprintln!("  packets {packets}, total audio {seconds:.1} s");
    eprintln!("  sample rates {rates:?}");
    eprintln!("  channels {channels:?}");
    eprintln!("  format tags {tags:?}");
    eprintln!("  parsed in {:.2} s", elapsed.as_secs_f64());

    assert_eq!(other_kind, 0, "some audio halves were not a RIFF we know");
    // Every LIP block frames and every version word reads 1, which is what
    // the reader promises. Header word 2 is not constant across the archive,
    // so the reader reports it rather than asserting a value. See
    // docs/audio.md, "What the LIP header actually holds".
    assert!(!lip_words.is_empty(), "no lip block was parsed at all");
}

#[test]
fn a_voice_line_sniffs_as_a_fuz_through_the_source_layer() {
    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let name = archive
        .files()
        .iter()
        .find(|n| n.to_lowercase().ends_with(".fuz"))
        .expect("at least one voice file")
        .clone();
    let bytes = archive.read(&name).expect("the file reads");
    assert_eq!(soe_audio::sniff(&bytes), Container::Fuz);
    let fuz = Fuz::split(&bytes).expect("it splits");
    assert_eq!(soe_audio::sniff(fuz.audio), Container::Xwma);
}

/// The decode path, which needs the off-by-default feature and the binary.
#[cfg(feature = "xwma-ffmpeg")]
#[test]
fn a_real_voice_line_decodes_to_samples_of_the_right_length() {
    use soe_audio::wma::Ffmpeg;

    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: the ffmpeg binary is not on the path");
        return;
    }

    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".fuz"))
        .take(8)
        .cloned()
        .collect();

    let mut lines_done = 0usize;
    for name in &names {
        let bytes = archive.read(name).expect("a voice file reads");
        let fuz = Fuz::split(&bytes).expect("it splits");
        let xwma = Xwma::parse(fuz.audio).expect("the container parses");
        let pcm = soe_audio::wma::decode_with(&decoder, &xwma).expect("it decodes");

        assert_eq!(pcm.sample_rate, xwma.format.sample_rate);
        assert_eq!(pcm.channels, xwma.format.channels);
        assert!(pcm.peak() > 0.0, "{name} decoded to silence");
        // The packet table predicts the length, so the decode agreeing with
        // it is the check that both readings of the file are right.
        let predicted = xwma.duration_seconds();
        let got = pcm.duration_seconds();
        assert!(
            (predicted - got).abs() < 0.06,
            "{name}: table says {predicted:.3} s, decode gave {got:.3} s"
        );
        eprintln!(
            "  {name}: {:.3} s, {} packets, peak {:.3}",
            got,
            xwma.packets(),
            pcm.peak()
        );
        lines_done += 1;
    }
    assert!(lines_done > 0);
}

/// Proves the open path end to end: take a real vanilla line, re-encode it to
/// Ogg Vorbis and Ogg Opus with ffmpeg the way a mod author would, and decode
/// both back through the pure Rust decoders.
#[cfg(feature = "xwma-ffmpeg")]
#[test]
fn the_same_line_re_encoded_open_decodes_back_through_the_rust_decoders() {
    use soe_audio::wma::Ffmpeg;
    use std::process::{Command, Stdio};

    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: the ffmpeg binary is not on the path");
        return;
    }

    let name = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".fuz"))
        .nth(3)
        .expect("a voice file")
        .clone();
    let bytes = archive.read(&name).expect("the file reads");
    let fuz = Fuz::split(&bytes).expect("it splits");
    let xwma = Xwma::parse(fuz.audio).expect("the container parses");
    let reference = soe_audio::wma::decode_with(&decoder, &xwma).expect("it decodes");

    for (codec, container) in [
        ("libvorbis", Container::OggVorbis),
        ("libopus", Container::OggOpus),
    ] {
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "xwma",
                "-i",
                "pipe:0",
                "-c:a",
                codec,
                "-f",
                "ogg",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("ffmpeg runs");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin");
            let _ = stdin.write_all(&xwma.to_bytes());
        }
        let out = child.wait_with_output().expect("ffmpeg finishes");
        if !out.status.success() {
            eprintln!(
                "skipping {codec}: this ffmpeg build cannot encode it: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            continue;
        }

        assert_eq!(soe_audio::sniff(&out.stdout), container, "{codec} sniff");
        let pcm = match container {
            Container::OggVorbis => soe_audio::vorbis::decode(&out.stdout),
            Container::OggOpus => soe_audio::opus::decode(&out.stdout),
            _ => unreachable!(),
        }
        .unwrap_or_else(|e| panic!("{codec} decode: {e}"));

        assert!(pcm.peak() > 0.0, "{codec} decoded to silence");
        let drift = (pcm.duration_seconds() - reference.duration_seconds()).abs();
        assert!(
            drift < 0.12,
            "{codec}: {:.3} s against the vanilla {:.3} s",
            pcm.duration_seconds(),
            reference.duration_seconds()
        );
        eprintln!(
            "  {codec}: {:.3} s at {} Hz, {} channels, peak {:.3}",
            pcm.duration_seconds(),
            pcm.sample_rate,
            pcm.channels,
            pcm.peak()
        );
    }
}

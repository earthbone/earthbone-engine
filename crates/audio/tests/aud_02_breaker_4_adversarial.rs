// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::Xwma;
use soe_audio::wma::{Native, WmaDecoder};

#[test]
fn aud_02_truncated_xwma_prefixes_return_errors_without_panicking() {
    let malformed = b"RIFF\x24\x00\x00\x00XWMAfmt ";
    for end in 0..malformed.len() {
        let result = std::panic::catch_unwind(|| Xwma::parse(&malformed[..end]));
        assert!(result.is_ok(), "prefix ending at {end} panicked");
        assert!(result.unwrap().is_err(), "prefix ending at {end} parsed");
    }

    let result = std::panic::catch_unwind(|| Native.decode_xwma(malformed));
    assert!(result.is_ok(), "decoder panicked on a truncated xWMA");
    assert!(
        result.unwrap().is_err(),
        "decoder accepted a truncated xWMA"
    );
}

#[cfg(feature = "xwma-ffmpeg")]
fn max_correlation(left: &[f32], right: &[f32]) -> f64 {
    let mut best = 0.0_f64;
    for offset in (-4096_isize..=4096).step_by(32) {
        let left_start = offset.max(0) as usize;
        let right_start = (-offset).max(0) as usize;
        let count = left
            .len()
            .saturating_sub(left_start)
            .min(right.len().saturating_sub(right_start));
        let (mut dot, mut left_energy, mut right_energy) = (0.0, 0.0, 0.0);
        for index in (0..count).step_by(8) {
            let a = f64::from(left[left_start + index]);
            let b = f64::from(right[right_start + index]);
            dot += a * b;
            left_energy += a * a;
            right_energy += b * b;
        }
        if left_energy > 0.0 && right_energy > 0.0 {
            best = best.max(dot / (left_energy * right_energy).sqrt());
        }
    }
    best
}

#[cfg(feature = "xwma-ffmpeg")]
#[test]
fn aud_02_vanilla_voice_sample_correlation_meets_the_predeclared_threshold() {
    use soe_audio::Fuz;
    use soe_audio::wma::decode_with;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    const MIN_CORRELATION: f64 = 0.8;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Voices_en0.bsa"))
        .expect("English voice archive opens");
    let names: Vec<_> = archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".fuz"))
        .take(8)
        .cloned()
        .collect();
    assert_eq!(names.len(), 8, "vanilla voice corpus has eight lines");

    for name in names {
        let bytes = archive.read(&name).expect("voice line reads");
        let fuz = Fuz::split(&bytes).expect("FUZ splits");
        let xwma = Xwma::parse(fuz.audio).expect("voice audio parses as xWMA");
        let native = decode_with(&Native, &xwma).unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "xwma",
                "-i",
                "pipe:0",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("ffmpeg starts");
        child
            .stdin
            .take()
            .expect("ffmpeg stdin exists")
            .write_all(&xwma.to_bytes())
            .expect("xWMA reaches ffmpeg");
        let output = child.wait_with_output().expect("ffmpeg completes");
        assert!(output.status.success(), "ffmpeg decodes {name}");
        let (whole, tail) = output.stdout.as_chunks::<4>();
        assert!(
            tail.is_empty(),
            "ffmpeg outputs complete samples for {name}"
        );
        let samples: Vec<_> = whole
            .iter()
            .map(|sample| f32::from_le_bytes(*sample))
            .collect();
        assert_eq!(
            native.samples.len(),
            samples.len(),
            "sample count for {name}"
        );
        let correlation = max_correlation(&native.samples, &samples);
        assert!(
            correlation >= MIN_CORRELATION,
            "{name}: correlation {correlation:.3}"
        );
    }
}

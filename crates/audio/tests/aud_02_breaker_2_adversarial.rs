// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::wma::{decode_with, Native};
use soe_audio::{Fuz, Xwma};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn ffmpeg_samples(xwma: &Xwma<'_>) -> Vec<f32> {
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
    assert!(
        output.status.success(),
        "ffmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (whole, tail) = output.stdout.as_chunks::<4>();
    assert!(tail.is_empty(), "ffmpeg returned partial float samples");
    whole
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect()
}

fn max_correlation(left: &[f32], right: &[f32]) -> f64 {
    let mut best = 0.0f64;
    for offset in -4096isize..=4096 {
        let left_start = offset.max(0) as usize;
        let right_start = (-offset).max(0) as usize;
        let compared = left
            .len()
            .saturating_sub(left_start)
            .min(right.len().saturating_sub(right_start));
        let (mut dot, mut left_energy, mut right_energy) = (0.0, 0.0, 0.0);
        for index in (0..compared).step_by(8) {
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

#[test]
fn aud_02_installed_vanilla_xwma_matches_ffmpeg_without_feature() {
    const MIN_WAVEFORM_CORRELATION: f64 = 0.8;
    const MAX_DURATION_ERROR_SECONDS: f64 = 0.06;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Voices_en0.bsa"))
        .expect("English voice archive opens");
    assert!(
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success()),
        "ffmpeg is available for the out-of-process reference"
    );
    let mut names: Vec<_> = archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".fuz"))
        .cloned()
        .collect();
    names.sort();
    let name = names.first().expect("archive contains a vanilla FUZ line");
    let bytes = archive.read(name).expect("vanilla voice line reads");
    let fuz = Fuz::split(&bytes).expect("vanilla FUZ splits");
    let xwma = Xwma::parse(fuz.audio).expect("vanilla xWMA parses");

    let native = decode_with(&Native, &xwma).unwrap_or_else(|error| panic!("{name}: {error}"));
    let repeated = decode_with(&Native, &xwma).unwrap_or_else(|error| panic!("{name}: {error}"));
    let reference = ffmpeg_samples(&xwma);
    assert_eq!(
        native.samples, repeated.samples,
        "native decode is repeatable"
    );
    assert_eq!(native.sample_rate, xwma.format.sample_rate);
    assert_eq!(native.channels, xwma.format.channels);
    assert_ne!(native.samples, [] as [f32; 0]);
    assert_ne!(reference, [] as [f32; 0]);
    let reference_duration = reference.len() as f64
        / f64::from(xwma.format.sample_rate)
        / f64::from(xwma.format.channels);
    assert!(
        (native.duration_seconds() - reference_duration).abs() <= MAX_DURATION_ERROR_SECONDS,
        "native duration {} differs from ffmpeg duration {reference_duration}",
        native.duration_seconds()
    );
    if native.peak() > 0.0 && reference.iter().any(|sample| *sample != 0.0) {
        let correlation = max_correlation(&native.samples, &reference);
        assert!(
            correlation >= MIN_WAVEFORM_CORRELATION,
            "{name}: native waveform correlation is {correlation:.3}"
        );
    } else {
        assert_eq!(native.peak(), 0.0);
        assert!(reference.iter().all(|sample| *sample == 0.0));
    }
}

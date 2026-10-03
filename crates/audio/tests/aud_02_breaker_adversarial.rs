// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::wma::{decode_with, Native};
use soe_audio::{Fuz, Xwma};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn voice_archive() -> Option<soe_bsa::Archive> {
    let data = PathBuf::from(std::env::var_os("SKYRIM_DATA")?);
    let path = data.join("Skyrim - Voices_en0.bsa");
    path.exists().then(|| soe_bsa::Archive::open(&path).expect("voice archive opens"))
}

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
    let (whole, _) = output.stdout.as_chunks::<4>();
    whole
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect()
}

fn max_correlation(left: &[f32], right: &[f32]) -> f64 {
    let mut best = 0.0f64;
    for offset in (-4096isize..=4096).step_by(32) {
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
fn aud_02_breaker_multiple_vanilla_lines_match_ffmpeg_and_repeat() {
    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA voice archive is unavailable");
        return;
    };
    if !Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        eprintln!("skipping: ffmpeg is unavailable");
        return;
    }

    let names: Vec<_> = archive
        .files()
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".fuz"))
        .cloned()
        .collect();
    assert!(!names.is_empty(), "English voice archive has no FUZ entries");
    let indices: Vec<usize> = (0..8)
        .map(|slot| slot * (names.len() - 1) / 7)
        .collect();
    let mut waveform_mismatches = Vec::new();
    for index in indices {
        let name = &names[index];
        let bytes = archive.read(name).expect("voice line reads");
        let fuz = Fuz::split(&bytes).expect("FUZ splits");
        let xwma = Xwma::parse(fuz.audio).expect("vanilla xWMA parses");
        let native = decode_with(&Native, &xwma).unwrap_or_else(|error| panic!("{name}: {error}"));
        let repeated = decode_with(&Native, &xwma).unwrap_or_else(|error| panic!("{name}: {error}"));
        let reference = ffmpeg_samples(&xwma);

        assert_eq!(native.sample_rate, xwma.format.sample_rate, "{name}");
        assert_eq!(native.channels, xwma.format.channels, "{name}");
        assert_eq!(native.samples, repeated.samples, "{name}: decode is repeatable");
        assert!(!native.samples.is_empty(), "{name}: native output is empty");
        assert!(!reference.is_empty(), "{name}: ffmpeg output is empty");
        let native_duration = native.duration_seconds();
        let reference_duration = reference.len() as f64
            / f64::from(xwma.format.sample_rate)
            / f64::from(xwma.format.channels);
        assert!(
            (native_duration - reference_duration).abs() < 0.06,
            "{name}: duration {native_duration:.3}s versus ffmpeg {reference_duration:.3}s"
        );
        if native.peak() > 0.0 && reference.iter().any(|sample| *sample != 0.0) {
            let correlation = max_correlation(&native.samples, &reference);
            if correlation < 0.8 {
                waveform_mismatches.push(format!("{name}: correlation {correlation:.3}"));
            }
        } else {
            assert_eq!(native.peak(), 0.0, "{name}: silence differs from ffmpeg");
            assert!(reference.iter().all(|sample| *sample == 0.0), "{name}: only one decoder is silent");
        }
    }
    assert!(
        waveform_mismatches.is_empty(),
        "native waveform correlation is below 0.8: {waveform_mismatches:?}"
    );
}

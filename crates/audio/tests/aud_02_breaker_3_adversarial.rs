// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::wma::{decode_with, Native, WmaDecoder};
use soe_audio::{Fuz, Xwma};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn archive() -> soe_bsa::Archive {
    let data = PathBuf::from(std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA is set"));
    soe_bsa::Archive::open(&data.join("Skyrim - Voices_en0.bsa")).expect("voice archive opens")
}

fn xwma_for(archive: &soe_bsa::Archive, name: &str) -> Vec<u8> {
    let bytes = archive.read(name).expect("voice entry reads");
    let fuz = Fuz::split(&bytes).expect("FUZ splits");
    Xwma::parse(fuz.audio)
        .expect("FUZ audio is xWMA")
        .to_bytes()
}

fn ffmpeg_samples(bytes: &[u8]) -> Vec<f32> {
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
        .write_all(bytes)
        .expect("xWMA reaches ffmpeg");
    let output = child.wait_with_output().expect("ffmpeg completes");
    assert!(output.status.success(), "ffmpeg decodes xWMA");
    let (whole, tail) = output.stdout.as_chunks::<4>();
    assert!(tail.is_empty(), "ffmpeg output contains complete float samples");
    whole
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect()
}

fn correlation(left: &[f32], right: &[f32]) -> f64 {
    let mut best = 0.0f64;
    for offset in (-4096isize..=4096).step_by(32) {
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

#[test]
fn aud_02_stereo_vanilla_line_preserves_interleaved_frames_against_ffmpeg() {
    let archive = archive();
    let stereo = archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".fuz"))
        .find_map(|name| {
            let bytes = archive.read(name).ok()?;
            let fuz = Fuz::split(&bytes).ok()?;
            let xwma = Xwma::parse(fuz.audio).ok()?;
            (xwma.format.channels == 2).then(|| (name.clone(), xwma.to_bytes()))
        })
        .expect("vanilla archive includes stereo xWMA voice lines");
    let (name, bytes) = stereo;
    let parsed = Xwma::parse(&bytes).expect("selected stereo xWMA parses");
    let native = decode_with(&Native, &parsed).unwrap_or_else(|error| panic!("{name}: {error}"));
    let reference = ffmpeg_samples(&bytes);

    assert_eq!(native.channels, 2, "{name}: stereo channel count is retained");
    assert_eq!(native.sample_rate, parsed.format.sample_rate, "{name}");
    assert_eq!(native.samples.len() % 2, 0, "{name}: samples contain whole frames");
    assert!(native.samples.iter().all(|sample| sample.is_finite()), "{name}");
    assert_eq!(native.samples.len(), reference.len(), "{name}: frame count matches ffmpeg");
    assert!(correlation(&native.samples, &reference) >= 0.8, "{name}: stereo waveform matches ffmpeg");
}

#[test]
fn aud_02_native_decoder_has_no_cross_stream_state_leak() {
    let archive = archive();
    let names: Vec<_> = archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".fuz"))
        .take(2)
        .cloned()
        .collect();
    assert_eq!(names.len(), 2, "voice archive has multiple lines");
    let first = xwma_for(&archive, &names[0]);
    let second = xwma_for(&archive, &names[1]);

    let before = Native.decode_xwma(&first).expect("first line decodes");
    let middle = Native.decode_xwma(&second).expect("second line decodes");
    let after = Native.decode_xwma(&first).expect("first line decodes again");

    assert!(!middle.samples.is_empty(), "second line has decoded samples");
    assert_eq!(before, after, "intervening stream does not change repeated decode");
}

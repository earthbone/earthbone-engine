// SPDX-License-Identifier: GPL-3.0-or-later

//! LIP weight decoding against authored matrices and the installed archive.
//! Zero-run fixtures include hand-written bytes independent of the decoder.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::lip::{
    COMPRESSED, GESTURE_CHANNELS, HAS_GESTURES, MAX_DECODED_BYTES, VARIABLE_TARGETS,
};
use soe_audio::{Error, Fuz, Lip, Unsupported};

fn block(flags: u32, expanded: usize, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [1, expanded as u32 + 16, flags] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(body);
    bytes
}

// CONTEXT: Three frames, starting at -1, with two targets. The final weight
// is negative zero so any normalization would lose its authored bit pattern.
const RAW: &[u8] = &[
    3, 0, 0, 0, 255, 255, 255, 255, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63, 0, 0, 128, 63, 0, 0, 128,
    62, 0, 0, 0, 0, 0, 0, 0, 128,
];

// CONTEXT: Runs cross metadata and float boundaries. In particular the run
// of ten zeros spans the target count, a whole weight and part of the next.
const PACKED: &[u8] = &[
    3, 0, 3, 0, 255, 255, 255, 255, 2, 0, 10, 0, 63, 0, 2, 0, 128, 63, 0, 2, 0, 128, 62, 0, 7, 0,
    128,
];

#[test]
fn aud_01_known_lip_files_decode_to_authored_weight_tracks() {
    let expected = [[0.0f32, 0.5], [1.0, 0.25], [0.0, -0.0]];
    for (flags, body) in [
        (VARIABLE_TARGETS, RAW),
        (VARIABLE_TARGETS | COMPRESSED, PACKED),
    ] {
        let bytes = block(flags, RAW.len(), body);
        let lip = Lip::parse(&bytes).unwrap();
        let track = lip.decode().unwrap();
        assert_eq!(lip.targets(), Some(2), "metadata uses the decoded body");
        assert_eq!(track.first_frame(), -1, "pre-roll survives decoding");
        assert_eq!(track.frame_count(), 3, "all stored frames survive decoding");
        assert_eq!(
            track.target_count(),
            2,
            "target count is read from metadata"
        );
        assert_eq!(track.gesture_count(), 0, "flags omit gesture channels");
        for (frame, want) in track.frames().zip(expected) {
            assert_eq!(
                frame
                    .targets
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                want.map(f32::to_bits),
                "every weight preserves its exact authored bits"
            );
            assert!(frame.gestures.is_empty(), "no invented gesture values");
        }
        assert_eq!(
            track.frame_at(-1).unwrap().targets,
            expected[0],
            "native first frame"
        );
        assert_eq!(
            track.frame_at(0).unwrap().targets,
            expected[1],
            "native audio origin"
        );
        for outside in [i64::MIN, -2, 2, i64::MAX] {
            assert!(
                track.frame_at(outside).is_none(),
                "frame {outside} lies outside the track"
            );
        }
    }
}

fn zero_pack(raw: &[u8]) -> Vec<u8> {
    let mut packed = Vec::new();
    let mut pending = 0u16;
    for byte in raw.iter().copied().chain(std::iter::once(1)) {
        if byte == 0 && pending != u16::MAX {
            pending += 1;
            continue;
        }
        if pending != 0 {
            packed.push(0);
            packed.extend_from_slice(&pending.to_le_bytes());
            pending = 0;
        }
        if byte == 0 {
            pending = 1;
        } else {
            packed.push(byte);
        }
    }
    packed.pop();
    packed
}

#[test]
fn gesture_channels_preserve_signed_and_above_unit_weights() {
    let mut raw = Vec::new();
    for word in [2u32, 0, 8] {
        raw.extend_from_slice(&word.to_le_bytes());
    }
    let targets = [0.0f32, 0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25];
    let gestures: Vec<f32> = (0..GESTURE_CHANNELS)
        .map(|i| i as f32 / 4.0 - 2.0)
        .collect();
    for _ in 0..2 {
        for weight in targets.iter().chain(&gestures) {
            raw.extend_from_slice(&weight.to_le_bytes());
        }
    }
    let bytes = block(
        COMPRESSED | VARIABLE_TARGETS | HAS_GESTURES,
        raw.len(),
        &zero_pack(&raw),
    );
    let track = Lip::parse(&bytes).unwrap().decode().unwrap();
    assert_eq!(
        track.gesture_count(),
        GESTURE_CHANNELS,
        "gesture flag adds 17 channels"
    );
    for frame in track.frames() {
        assert_eq!(frame.targets, targets, "mouth target order stays intact");
        assert_eq!(
            frame.gestures, gestures,
            "gesture values must not be clamped"
        );
    }
}

#[test]
fn long_zero_runs_cross_the_u16_limit_without_losing_frames() {
    let mut raw = Vec::new();
    for word in [6000u32, 0, 8] {
        raw.extend_from_slice(&word.to_le_bytes());
    }
    raw.resize(12 + 6000 * 8 * 4, 0);
    let bytes = block(COMPRESSED | VARIABLE_TARGETS, raw.len(), &zero_pack(&raw));
    let track = Lip::parse(&bytes).unwrap().decode().unwrap();
    assert_eq!(track.frame_count(), 6000, "all silent frames are retained");
    assert!(
        track.frames().all(|f| f.targets.iter().all(|v| *v == 0.0)),
        "runs expand to zero"
    );
}

#[test]
fn every_truncation_of_a_valid_track_is_rejected() {
    for (flags, body) in [
        (VARIABLE_TARGETS, RAW),
        (VARIABLE_TARGETS | COMPRESSED, PACKED),
    ] {
        let bytes = block(flags, RAW.len(), body);
        for end in 0..bytes.len() {
            assert!(
                Lip::parse(&bytes[..end])
                    .and_then(|lip| lip.decode())
                    .is_err(),
                "prefix length {end} with flags {flags} must not decode"
            );
        }
    }
}

#[test]
fn zero_length_and_oversized_zero_runs_are_rejected() {
    for (body, message) in [
        (&[0, 0, 0][..], "zero-length"),
        (&[0, 255, 255][..], "exceeds"),
    ] {
        let bytes = block(COMPRESSED | VARIABLE_TARGETS, RAW.len(), body);
        let error = Lip::parse(&bytes).unwrap().decode().unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn metadata_must_match_the_declared_expansion_before_allocating() {
    for (at, value) in [(0, u32::MAX), (8, 0), (8, u32::MAX)] {
        let mut body = RAW.to_vec();
        body[at..at + 4].copy_from_slice(&value.to_le_bytes());
        let bytes = block(VARIABLE_TARGETS, RAW.len(), &body);
        assert!(
            matches!(
                Lip::parse(&bytes).unwrap().decode().unwrap_err(),
                Error::Malformed { .. }
            ),
            "invalid dimensions must be a typed error"
        );
    }
    let bytes = block(VARIABLE_TARGETS, MAX_DECODED_BYTES + 1, RAW);
    assert!(
        Lip::parse(&bytes)
            .unwrap()
            .decode()
            .unwrap_err()
            .to_string()
            .contains("out of bounds"),
        "the declared size must not cause unbounded allocation"
    );
    let bytes = block(VARIABLE_TARGETS, RAW.len() + 4, RAW);
    assert!(
        Lip::parse(&bytes).unwrap().decode().is_err(),
        "declared size must match dimensions"
    );
}

#[test]
fn nonfinite_weights_are_rejected_without_clamping() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut body = RAW.to_vec();
        body[12..16].copy_from_slice(&value.to_le_bytes());
        let bytes = block(VARIABLE_TARGETS, body.len(), &body);
        assert!(
            Lip::parse(&bytes)
                .unwrap()
                .decode()
                .unwrap_err()
                .to_string()
                .contains("non-finite"),
            "{value} is not a weight"
        );
    }
}

#[test]
fn trailing_encoded_bytes_are_rejected() {
    for tail in [&[1][..], &[0, 1, 0][..]] {
        let mut body = PACKED.to_vec();
        body.extend_from_slice(tail);
        let bytes = block(COMPRESSED | VARIABLE_TARGETS, RAW.len(), &body);
        assert!(
            Lip::parse(&bytes)
                .unwrap()
                .decode()
                .unwrap_err()
                .to_string()
                .contains("trailing"),
            "bytes outside the declared expansion must not disappear"
        );
    }
}

#[test]
fn unknown_fixed_target_and_big_endian_layouts_are_explicitly_unsupported() {
    for flags in [0, 1, 4, 5, 10, 11, 0x8000_0009] {
        let bytes = block(flags, RAW.len(), RAW);
        assert!(
            matches!(Lip::parse(&bytes).unwrap().decode().unwrap_err(),
            Error::Unsupported(Unsupported::LipFlags(value)) if value == flags),
            "flags {flags:#x} must not be read as the known layout"
        );
    }
}

#[test]
fn installed_lip_bodies_decode_every_frame_and_target() {
    use std::io::{Read, Seek, SeekFrom};
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping LIP corpus: SKYRIM_DATA is unset");
        return;
    };
    let path = std::path::Path::new(&data).join("Skyrim - Voices_en0.bsa");
    let index = soe_bsa::index_archive(&path).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let (mut decoded, mut creature, mut humanoid, mut empty) = (0usize, 0usize, 0usize, 0usize);
    let (mut frames, mut samples) = (0u64, 0u64);
    for record in &index.records {
        if !std::path::Path::new(&record.path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("fuz"))
        {
            continue;
        }
        assert!(
            !record.compressed && !record.name_embedded,
            "unexpected archive packing"
        );
        file.seek(SeekFrom::Start(record.offset)).unwrap();
        let mut bytes = vec![0; record.raw_size as usize];
        file.read_exact(&mut bytes).unwrap();
        let fuz = Fuz::split(&bytes).unwrap();
        if fuz.lip.is_empty() {
            empty += 1;
            continue;
        }
        let lip = Lip::parse(fuz.lip).unwrap();
        let track = lip
            .decode()
            .unwrap_or_else(|error| panic!("{}: {error}", record.path));
        let (targets, gestures) = if lip.has_gestures() {
            humanoid += 1;
            (16, 17)
        } else {
            creature += 1;
            (8, 0)
        };
        assert_eq!(
            (track.target_count(), track.gesture_count()),
            (targets, gestures),
            "{}: corpus channel shape",
            record.path
        );
        assert_eq!(
            lip.targets(),
            Some(targets as u16),
            "{}: target metadata",
            record.path
        );
        assert_eq!(
            track.frames().len(),
            track.frame_count() as usize,
            "all frames decoded"
        );
        assert_eq!(
            lip.size as u64,
            28 + u64::from(track.frame_count()) * (targets + gestures) as u64 * 4,
            "{}: expanded size",
            record.path
        );
        frames += u64::from(track.frame_count());
        samples += u64::from(track.frame_count()) * (targets + gestures) as u64;
        decoded += 1;
    }
    eprintln!("LIP corpus: {decoded} decoded, {empty} empty, {humanoid} humanoid, {creature} creature, {frames} frames, {samples} samples");
    assert_eq!(
        (decoded, empty, humanoid, creature),
        (74_070, 1_338, 73_118, 952),
        "every installed English LIP body must decode"
    );
}

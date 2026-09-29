// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::viseme::{MouthCue, Shape, VisemeTrack};
use soe_esm::plugin::Record;
use soe_esm::records::race::{Race, PHONEMES};
use soe_esm::tag;
use soe_godot_host::facial::sample_shape_weights;
use soe_nif::parse_head_tri;

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tri() -> soe_nif::tri::HeadTri {
    let mut out = b"FRTRI003".to_vec();
    for value in [3, 1, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0] {
        u32le(&mut out, value);
    }
    for vertex in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for component in vertex {
            out.extend_from_slice(&component.to_le_bytes());
        }
    }
    for index in [0, 1, 2] {
        u32le(&mut out, index);
    }
    out.extend_from_slice(&[0; 12]);
    for (name, delta) in [
        ("second", [[0, 0, 0], [0, 0, 200], [0, 0, 0]]),
        ("first", [[0, 100, 0], [0, 0, 0], [0, 0, 0]]),
    ] {
        u32le(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&0.01f32.to_le_bytes());
        for [x, y, z] in delta {
            i16le(&mut out, x);
            i16le(&mut out, y);
            i16le(&mut out, z);
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    parse_head_tri(&out).unwrap()
}

fn race(aa: [f32; 2], eh: [f32; 2], sil: [f32; 2]) -> Race {
    let mut record = Record::new(tag::RACE, 1)
        .zsub(b"PHTN", "first")
        .zsub(b"PHTN", "second");
    for phoneme in PHONEMES {
        let row = match phoneme {
            "AA" => aa,
            "EH" => eh,
            "SIL" => sil,
            _ => [0.0, 0.0],
        };
        record = record.sub(
            b"PHWT",
            row.into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    Race::from_record(&record).unwrap()
}

#[test]
fn crossfade_uses_phoneme_rows_and_matches_morphs_by_name() {
    let tri = tri();
    let race = race([1.0, 0.0], [0.0, 1.0], [0.0, 0.0]);
    let pose = sample_shape_weights(&[(Shape::D, 0.25), (Shape::C, 0.75)], &race, &tri).unwrap();

    assert_eq!(
        pose.weights,
        [("first".into(), 0.25), ("second".into(), 0.75)]
    );
    assert_eq!(pose.positions[0], [0.0, 0.25, 0.0]);
    assert_eq!(pose.positions[1], [1.0, 0.0, 1.5]);
}

#[test]
fn every_sample_starts_at_base_and_idle_uses_the_authored_sil_row() {
    let tri = tri();
    let race = race([1.0, 0.0], [0.0, 1.0], [0.0, 0.0]);
    let track = VisemeTrack {
        duration: 1.0,
        cues: vec![MouthCue {
            start: 0.0,
            end: 1.0,
            shape: Shape::D,
        }],
    };

    let speaking =
        soe_godot_host::facial::sample_facial_pose(&track, 0.0, 0.0, &race, &tri).unwrap();
    assert_eq!(speaking.positions[0], [0.0, 1.0, 0.0]);
    let neutral_again =
        soe_godot_host::facial::sample_facial_pose(&track, 4.0, 0.0, &race, &tri).unwrap();
    assert_ne!(neutral_again.positions, speaking.positions);
    assert_eq!(neutral_again.positions, tri.vertices());
}

#[test]
fn a_declared_target_without_a_tri_morph_keeps_its_weight_without_moving_vertices() {
    let tri = tri();
    let mut record = Record::new(tag::RACE, 2).zsub(b"PHTN", "missing");
    for phoneme in PHONEMES {
        let weight = if phoneme == "AA" { 0.75f32 } else { 0.0 };
        record = record.sub(b"PHWT", weight.to_le_bytes());
    }
    let race = Race::from_record(&record).unwrap();
    let pose = sample_shape_weights(&[(Shape::D, 1.0)], &race, &tri).unwrap();

    assert_eq!(pose.weights, [("missing".into(), 0.75)]);
    assert_eq!(pose.positions, tri.vertices());
}

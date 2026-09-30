// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::viseme::{MouthCue, Shape, VisemeTrack};
use soe_esm::plugin::Record;
use soe_esm::records::race::{Race, PHONEMES};
use soe_esm::tag;
use soe_godot_host::facial::sample_facial_pose;
use soe_nif::parse_head_tri;

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn synthetic_head_tri() -> Vec<u8> {
    let mut out = b"FRTRI003".to_vec();
    for value in [3, 1, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0] {
        u32le(&mut out, value);
    }
    for vertex in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for component in vertex {
            out.extend_from_slice(&component.to_le_bytes());
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 1);
    u32le(&mut out, 2);
    out.extend_from_slice(&[0; 12]);

    for (name, delta) in [
        ("lift", [[0, 0, 0], [0, 0, 100], [0, 0, 0]]),
        ("other", [[0, 100, 0], [0, 0, 0], [0, 0, 0]]),
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
    out
}

fn race() -> Race {
    let mut record = Record::new(tag::RACE, 1)
        .zsub(tag::EDID, "VisemeTestRace")
        .zsub(b"PHTN", "lift")
        .zsub(b"PHTN", "other");
    for phoneme in PHONEMES {
        let row = if phoneme == "AA" {
            [1.0f32, 0.0]
        } else {
            [0.0f32, 0.0]
        };
        let bytes: Vec<u8> = row.into_iter().flat_map(f32::to_le_bytes).collect();
        record = record.sub(b"PHWT", bytes);
    }
    Race::from_record(&record).expect("synthetic race record parses")
}

#[test]
fn anm_13_viseme_sequence_drives_race_declared_morphs_and_returns_to_neutral() {
    let track = VisemeTrack {
        duration: 0.8,
        cues: vec![
            MouthCue {
                start: 0.0,
                end: 0.4,
                shape: Shape::D,
            },
            MouthCue {
                start: 0.4,
                end: 0.8,
                shape: Shape::X,
            },
        ],
    };
    let race = race();
    let tri = parse_head_tri(&synthetic_head_tri()).expect("synthetic head TRI parses");

    let speaking =
        sample_facial_pose(&track, 0.2, 0.0, &race, &tri).expect("authored race target applies");
    assert_eq!(
        speaking.weights,
        [("lift".to_owned(), 1.0), ("other".to_owned(), 0.0)]
    );
    assert_eq!(speaking.positions[1], [1.0, 0.0, 1.0]);
    assert_eq!(speaking.positions[2], tri.vertices()[2]);

    let neutral = sample_facial_pose(&track, 0.6, 0.0, &race, &tri).expect("rest cue applies");
    assert_eq!(
        neutral.weights,
        [("lift".to_owned(), 0.0), ("other".to_owned(), 0.0)]
    );
    assert_eq!(neutral.positions, tri.vertices());
}

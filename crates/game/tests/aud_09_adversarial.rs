// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ambience, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x800;
const REGION: u32 = 0x801;
const MUSIC: u32 = 0x802;
const TRACK: u32 = 0x803;
const OTHER: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameter: u32) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn world(track: Record, region_music: Option<&[u8]>) -> World {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "aud09 adversarial".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"CELL"), CELL)
            .sub(T(b"DATA"), [1u8])
            .sub(T(b"XCLR"), REGION.to_le_bytes()),
    );
    let mut rdat = 7u32.to_le_bytes().to_vec();
    rdat.extend_from_slice(&[0, 1, 0, 0]);
    let mut region = Record::new(T(b"REGN"), REGION).sub(T(b"RDAT"), rdat);
    if let Some(raw) = region_music {
        region = region.sub(T(b"RDMO"), raw);
    } else {
        region = region.sub(T(b"RDMO"), MUSIC.to_le_bytes());
    }
    plugin.push_top(region);
    plugin.push_top(
        Record::new(T(b"MUSC"), MUSIC)
            .sub(T(b"FNAM"), 0u32.to_le_bytes())
            .sub(T(b"TNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(track);
    plugin.push_top(Record::new(T(b"STAT"), OTHER));
    let mut order = LoadOrder::new();
    order.add("Aud09.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn aud_09_condition_false_track_is_not_started() {
    let track = Record::new(T(b"MUST"), TRACK)
        .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
        .zsub(T(b"ANAM"), "Music/Conditional.wav")
        .sub(T(b"CTDA"), condition(72, OTHER));
    let mut world = world(track, None);
    assert!(ambience::tracks_for_cell(&mut world, id(CELL))
        .unwrap()
        .is_empty());
}

#[test]
fn aud_09_palette_resolves_its_eligible_single_track_leaf() {
    let palette = Record::new(T(b"MUST"), TRACK)
        .sub(T(b"CNAM"), 0x23F6_78C3_u32.to_le_bytes())
        .sub(T(b"SNAM"), OTHER.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "aud09 palette".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"CELL"), CELL)
            .sub(T(b"DATA"), [1u8])
            .sub(T(b"XCMO"), MUSIC.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUSC"), MUSIC)
            .sub(T(b"FNAM"), 0u32.to_le_bytes())
            .sub(T(b"TNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(palette);
    plugin.push_top(
        Record::new(T(b"MUST"), OTHER)
            .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
            .zsub(T(b"ANAM"), "Music/Leaf.wav"),
    );
    let mut order = LoadOrder::new();
    order.add("Aud09.esm", plugin).unwrap();
    let mut world = World::new(order);
    let tracks = ambience::tracks_for_cell(&mut world, id(CELL)).unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.path.as_str())
            .collect::<Vec<_>>(),
        ["Music/Leaf.wav"]
    );
}

#[test]
fn aud_09_malformed_fixed_size_region_music_form_is_reported() {
    let track = Record::new(T(b"MUST"), TRACK)
        .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
        .zsub(T(b"ANAM"), "Music/Track.wav");
    let mut world = world(track, Some(&[0x02, 0x08]));
    assert!(ambience::tracks_for_cell(&mut world, id(CELL)).is_err());
}

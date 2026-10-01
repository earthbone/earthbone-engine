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

fn order(cell_music: Option<u32>, track_type: u32) -> World {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "aud09".into(),
        ..Default::default()
    });
    let mut cell = Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8]);
    if let Some(music) = cell_music {
        cell = cell.sub(T(b"XCMO"), music.to_le_bytes());
    }
    cell = cell.sub(T(b"XCLR"), REGION.to_le_bytes());
    plugin.push_top(cell);
    let mut rdat = 7u32.to_le_bytes().to_vec();
    rdat.extend_from_slice(&[0, 1, 0, 0]);
    plugin.push_top(
        Record::new(T(b"REGN"), REGION)
            .sub(T(b"RDAT"), rdat)
            .sub(T(b"RDMO"), MUSIC.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUSC"), MUSIC)
            .sub(T(b"FNAM"), 0u32.to_le_bytes())
            .sub(T(b"TNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), TRACK)
            .sub(T(b"CNAM"), track_type.to_le_bytes())
            .zsub(T(b"ANAM"), "Music\\Test.wav"),
    );
    let mut order = LoadOrder::new();
    order.add("Aud09.esm", plugin).unwrap();
    World::new(order)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn aud_09_cell_music_starts_eligible_single_track() {
    let mut world = order(Some(MUSIC), 0x6ED7_E048);
    let tracks = ambience::tracks_for_cell(&mut world, id(CELL)).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].cell, id(CELL));
    assert_eq!(tracks[0].descriptor, id(MUSIC));
    assert_eq!(tracks[0].path, "Music\\Test.wav");
}

#[test]
fn aud_09_region_music_starts_when_cell_has_no_music() {
    let mut world = order(None, 0x6ED7_E048);
    let tracks = ambience::tracks_for_cell(&mut world, id(CELL)).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].descriptor, id(MUSIC));
}

#[test]
fn aud_09_palette_and_silent_tracks_are_not_started_as_files() {
    let mut world = order(Some(MUSIC), 0x23F6_78C3);
    assert!(ambience::tracks_for_cell(&mut world, id(CELL))
        .unwrap()
        .is_empty());
}

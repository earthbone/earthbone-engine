// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{music, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const MUSIC: u32 = 0x800;
const TRACK: u32 = 0x801;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_track(track: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"MUSC"), MUSIC)
            .sub(T(b"FNAM"), 0u32.to_le_bytes())
            .sub(T(b"PNAM"), [1, 0, 0, 0])
            .sub(T(b"TNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(track);
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn aud_05_missing_music_record_is_rejected() {
    let mut game = Game::new(LoadOrder::new());
    assert!(music::select(&mut game, id(MUSIC), id(1), 0.5).is_err());
}

#[test]
fn aud_05_malformed_track_type_is_rejected_before_playback() {
    let track = Record::new(T(b"MUST"), TRACK)
        .sub(T(b"CNAM"), [0x48, 0xe0])
        .zsub(T(b"ANAM"), "Sound/Music/bad.wav");
    let mut game = game_with_track(track);
    assert!(music::select(&mut game, id(MUSIC), id(1), 0.5).is_err());
}

#[test]
fn aud_05_silent_leaf_is_selected_without_a_filename() {
    let track = Record::new(T(b"MUST"), TRACK).sub(T(b"CNAM"), 0xA1A9_C4D5_u32.to_le_bytes());
    let mut game = game_with_track(track);
    let selected = music::select(&mut game, id(MUSIC), id(1), 0.5).unwrap();
    assert_eq!(selected.track, id(TRACK));
    assert_eq!(selected.path, None);
}

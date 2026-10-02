// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{music, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const MUSIC: u32 = 0x800;
const PALETTE: u32 = 0x801;
const INELIGIBLE: u32 = 0x802;
const TRACK: u32 = 0x803;
const SUBJECT: u32 = 0x804;
const OTHER: u32 = 0x805;
const BASE: u32 = 0x806;

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

fn test_game() -> Game {
    test_game_with(0x6f)
}

fn test_game_with(music_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"MUSC"), MUSIC)
            .sub(T(b"FNAM"), music_flags.to_le_bytes())
            .sub(T(b"PNAM"), [7, 0, 45, 0])
            .sub(T(b"WNAM"), 2.5_f32.to_le_bytes())
            .sub(T(b"TNAM"), PALETTE.to_le_bytes())
            .sub(T(b"TNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), PALETTE)
            .sub(T(b"CNAM"), 0x23F6_78C3_u32.to_le_bytes())
            .sub(T(b"SNAM"), INELIGIBLE.to_le_bytes())
            .sub(T(b"SNAM"), TRACK.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), INELIGIBLE)
            .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
            .zsub(T(b"ANAM"), "Sound/Music/Excluded.xwm")
            .sub(T(b"CTDA"), condition(72, OTHER)),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), TRACK)
            .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
            .sub(T(b"FLTV"), 180.0_f32.to_le_bytes())
            .sub(T(b"DNAM"), 4.0_f32.to_le_bytes())
            .zsub(T(b"ANAM"), "Sound\\Music\\Battle.xwm")
            .sub(T(b"LNAM"), [0, 0, 0, 0, 0, 0, 0x20, 0x41, 3, 0, 0, 0])
            .sub(T(b"FNAM"), [0, 0, 0xA0, 0x40])
            .sub(T(b"CTDA"), condition(72, BASE)),
    );
    plugin.push_top(Record::new(T(b"STAT"), OTHER));
    plugin.push_top(Record::new(T(b"STAT"), BASE));
    plugin.push_top(Record::new(T(b"REFR"), SUBJECT).sub(T(b"NAME"), BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn aud_05_music_selects_only_eligible_tracks_and_returns_authored_fades() {
    let mut game = test_game();
    let selected = music::select(&mut game, id(MUSIC), id(SUBJECT), 1.0).unwrap();
    assert_eq!(selected.track, id(TRACK));
    assert_eq!(selected.path.as_deref(), Some("sound/music/battle.xwm"));
    assert_eq!((selected.fade_in, selected.fade_out), (0.0, 4.0));
    assert_eq!(selected.duration, 180.0);
    assert_eq!(selected.loop_begin, Some(0.0));
    assert_eq!(selected.loop_end, Some(10.0));
    assert_eq!(selected.loop_count, 3);
    assert_eq!(selected.cue_points, [5.0]);
    assert!(selected.flags.plays_one_selection);
    assert!(selected.flags.abrupt_transition);
    assert!(selected.flags.cycle_tracks);
    assert!(selected.flags.maintain_track_order);
    assert!(selected.flags.ducks_current_track);
    assert!(selected.flags.does_not_queue);
    assert_eq!((selected.priority, selected.ducking_db), (7, 0.45));
}

#[test]
fn aud_05_music_rejects_missing_eligible_tracks_and_bad_draws() {
    let mut game = test_game();
    assert!(music::select(&mut game, id(MUSIC), id(0xDEAD), 0.0)
        .unwrap_err()
        .to_string()
        .contains("no eligible tracks"));
    assert!(music::select(&mut game, id(MUSIC), id(SUBJECT), f32::NAN).is_err());
}

#[test]
fn aud_05_music_uses_the_music_type_fade_when_transition_is_not_abrupt() {
    let mut game = test_game_with(0x6d);
    let selected = music::select(&mut game, id(MUSIC), id(SUBJECT), 1.0).unwrap();
    assert_eq!(selected.fade_in, 2.5);
}

#[test]
fn aud_05_music_selects_a_vanilla_music_type_when_game_data_is_installed() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping vanilla music survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path = data.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut game = Game::new(order);
    let subject = game.player();
    let music_types: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"MUSC"))
        .map(|(id, _)| id)
        .collect();
    let mut selected = None;
    for music_type in music_types {
        match music::select(&mut game, music_type, subject, 0.5) {
            Ok(track) => {
                selected = Some(track);
                break;
            }
            Err(error)
                if error.to_string().contains("no eligible tracks")
                    || error
                        .to_string()
                        .contains("unsupported condition function 707") => {}
            Err(error) => panic!("vanilla MUSC {music_type}: {error}"),
        }
    }
    assert!(
        selected.is_some(),
        "no installed MUSC had a supported eligible track"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-11 player sleep at active furniture markers.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const BED: u32 = 0x200;
const BASE: u32 = 0x300;
const SLEEP: u16 = 2;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(active_markers: u32, marker_type: u16) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"FURN"), BASE)
            .sub(Tag::new(b"MNAM"), active_markers.to_le_bytes())
            .sub(
                Tag::new(b"FNPR"),
                [marker_type as u8, (marker_type >> 8) as u8, 1, 0],
            ),
    );
    plugin.push_top(Record::new(Tag::new(b"REFR"), BED).sub(Tag::new(b"NAME"), BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_11_permitted_sleep_advances_game_time_and_releases_bed_on_wakeup() {
    let mut game = game(1, SLEEP);
    let player = game.player();
    let before = game.world().clock.game_days;

    assert!(game.sleep_player_at_furniture(id(BED), 8.0, false));

    assert!((game.world().clock.game_days - before - 1.0 / 3.0).abs() < 1.0e-9);
    assert_eq!(game.world().furniture_seat(player), None);
    assert!(!game.world().furniture_marker_in_use(id(BED), Some(0)));
}

#[test]
fn hst_11_interrupted_sleep_releases_bed_reservation() {
    let mut game = game(1, SLEEP);
    let player = game.player();

    assert!(game.sleep_player_at_furniture(id(BED), 1.0, true));

    assert_eq!(game.world().furniture_seat(player), None);
    assert!(!game.world().furniture_marker_in_use(id(BED), Some(0)));
}

#[test]
fn hst_11_sleep_requires_an_active_fnpr_sleep_marker() {
    let mut game = game(0, SLEEP);
    let before = game.world().clock.game_days;
    let player = game.player();

    assert!(!game.sleep_player_at_furniture(id(BED), 8.0, false));

    assert_eq!(game.world().clock.game_days, before);
    assert_eq!(game.world().furniture_seat(player), None);
}

#[test]
fn hst_11_non_sleep_furniture_marker_cannot_advance_time() {
    let mut game = game(1, 1);
    let before = game.world().clock.game_days;

    assert!(!game.sleep_player_at_furniture(id(BED), 8.0, false));

    assert_eq!(game.world().clock.game_days, before);
}

#[test]
fn hst_11_paused_clock_rejects_sleep_without_reserving_bed() {
    let mut game = game(1, SLEEP);
    game.world_mut().clock.timescale = 0.0;
    let before = game.world().clock.game_days;
    let player = game.player();

    assert!(!game.sleep_player_at_furniture(id(BED), 8.0, false));

    assert_eq!(game.world().clock.game_days, before);
    assert_eq!(game.world().furniture_seat(player), None);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const BED: u32 = 0x200;
const BASE: u32 = 0x300;
const SLEEP: u16 = 2;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_markers(active: u32, marker_types: &[u16]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut furniture = Record::new(Tag::new(b"FURN"), BASE).sub(b"MNAM", active.to_le_bytes());
    for marker_type in marker_types {
        furniture = furniture.sub(
            b"FNPR",
            [marker_type.to_le_bytes(), 1_u16.to_le_bytes()].concat(),
        );
    }
    plugin.push_top(furniture);
    plugin.push_top(Record::new(Tag::new(b"REFR"), BED).sub(b"NAME", BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_11_rejects_nonpositive_and_nonfinite_sleep_without_advancing_or_reserving() {
    for elapsed in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut game = game_with_markers(1, &[SLEEP]);
        let before = game.world().clock.game_days;
        let player = game.player();

        assert!(!game.sleep_player_at_furniture(id(BED), elapsed, false));

        assert_eq!(game.world().clock.game_days, before);
        assert_eq!(game.world().furniture_seat(player), None);
    }
}

#[test]
fn hst_11_uses_fnpr_index_as_marker_index_through_marker_23() {
    let mut markers = vec![1; 23];
    markers.push(SLEEP);
    let mut game = game_with_markers(1 << 23, &markers);
    let player = game.player();

    assert!(game.world_mut().can_sleep_at_furniture(id(BED)));
    assert!(game.sleep_player_at_furniture(id(BED), 1.0, false));
    assert_eq!(game.world().furniture_seat(player), None);
}

#[test]
fn hst_11_missing_reference_and_missing_furniture_record_do_not_advance_time() {
    let mut game = game_with_markers(1, &[SLEEP]);
    let before = game.world().clock.game_days;

    assert!(!game.sleep_player_at_furniture(id(BED + 1), 1.0, false));
    assert!(!game.sleep_player_at_furniture(id(BASE), 1.0, false));

    assert_eq!(game.world().clock.game_days, before);
}

#[test]
fn hst_11_truncated_fnpr_does_not_define_a_sleep_marker() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"FURN"), BASE)
            .sub(b"MNAM", 1_u32.to_le_bytes())
            .sub(b"FNPR", SLEEP.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag::new(b"REFR"), BED).sub(b"NAME", BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let before = game.world().clock.game_days;

    assert!(!game.sleep_player_at_furniture(id(BED), 1.0, false));
    assert_eq!(game.world().clock.game_days, before);
}

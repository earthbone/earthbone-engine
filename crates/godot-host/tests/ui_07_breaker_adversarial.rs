// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};
use soe_godot_host::race_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn race(id: u32, data: Vec<u8>) -> Record {
    Record::new(T(b"RACE"), id).zsub(T(b"FULL"), &format!("Race {id}"))
        .sub(T(b"DATA"), data)
}

fn game_with_races(races: impl IntoIterator<Item = Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in races {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Ui07Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn game_with_player_and_races(races: impl IntoIterator<Item = Record>, player_race: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in races {
        plugin.push_top(record);
    }
    plugin.push_top(
        npc::build_record(0x14, "Player", "Player", Config::default(), Stats::default())
            .sub(T(b"RNAM"), player_race.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ui07Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn playable_data() -> Vec<u8> {
    let mut data = vec![0; 124];
    data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    data[36..40].copy_from_slice(&100.0_f32.to_le_bytes());
    data[40..44].copy_from_slice(&100.0_f32.to_le_bytes());
    data[44..48].copy_from_slice(&100.0_f32.to_le_bytes());
    data[48..52].copy_from_slice(&300.0_f32.to_le_bytes());
    data[52..56].copy_from_slice(&1.0_f32.to_le_bytes());
    data
}

#[test]
fn ui_07_playable_flag_does_not_offer_truncated_race_data() {
    let mut data = vec![0; 36];
    data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    let game = game_with_races([race(0x700, data)]);

    assert!(race_menu::entries(&game).is_empty());
}

#[test]
fn ui_07_non_finite_required_race_attributes_are_not_selectable() {
    let mut data = playable_data();
    data[36..40].copy_from_slice(&f32::NAN.to_le_bytes());
    let game = game_with_races([race(0x701, data)]);

    assert!(race_menu::entries(&game).is_empty());
}

#[test]
fn ui_07_trait_setter_rejects_playable_but_unbuildable_race() {
    let mut data = vec![0; 36];
    data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    let valid = race(0x700, playable_data());
    let mut game = game_with_player_and_races([valid, race(0x702, data)], 0x700);
    let player = GlobalFormId {
        file: 0,
        object: 0x14,
    };

    let initial_traits = game.world_mut().player_character_traits();
    assert_eq!(
        initial_traits,
        Some((
            GlobalFormId {
                file: 0,
                object: 0x700,
            },
            false,
        ))
    );
    assert!(game
        .world_mut()
        .set_player_character_traits(
            GlobalFormId {
                file: 0,
                object: 0x702,
            },
            false,
        )
        .is_err());
    assert_eq!(
        game.world_mut().player_character_traits(),
        Some((
            GlobalFormId {
                file: 0,
                object: 0x700,
            },
            false,
        ))
    );
}

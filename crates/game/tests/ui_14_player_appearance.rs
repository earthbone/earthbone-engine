// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut race_data = vec![0; 128];
    race_data[32..36].copy_from_slice(&1u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x710)
            .zsub(T(b"FULL"), "Test Hair")
            .sub(T(b"DATA"), [5_u8])
            .sub(T(b"PNAM"), 3_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x711)
            .sub(T(b"DATA"), [3_u8])
            .sub(T(b"PNAM"), 3_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x712)
            .sub(T(b"DATA"), [5_u8])
            .sub(T(b"PNAM"), 2_u32.to_le_bytes()),
    );
    plugin.push_top(
        npc::build_record(
            0x14,
            "Player",
            "Player",
            Config {
                flags: npc::flags::FEMALE,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ui14.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_14_head_part_and_morph_selection_persists_with_player_appearance() {
    let mut game = game();
    {
        let mut world = game.world_mut();
        assert_eq!(world.player_character_traits(), Some((id(0x700), true)));
        world.set_player_head_part(3, id(0x710)).unwrap();
        world.set_player_face_morph("Nose", 0.5).unwrap();
        let appearance = world.player_appearance().unwrap();
        assert_eq!(appearance.head_parts.get(&3), Some(&id(0x710)));
        assert_eq!(appearance.morphs.get("Nose"), Some(&0.5));
    }
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    let appearance = game.world_mut().player_appearance().unwrap();
    assert_eq!(appearance.head_parts.get(&3), Some(&id(0x710)));
    assert_eq!(appearance.morphs.get("Nose"), Some(&0.5));
}

#[test]
fn ui_14_rejects_incompatible_head_parts_and_out_of_range_morphs_without_mutation() {
    let game = game();
    let mut world = game.world_mut();
    assert!(world.set_player_head_part(3, id(0x711)).is_err());
    assert!(world.set_player_head_part(3, id(0x712)).is_err());
    assert!(world.set_player_head_part(3, id(0x7ff)).is_err());
    assert!(world.set_player_face_morph("Nose", 1.1).is_err());
    assert!(world.set_player_face_morph("Nose", f32::NAN).is_err());
    assert_eq!(
        world.player_appearance().unwrap(),
        soe_game::appearance::PlayerAppearance::default()
    );
}

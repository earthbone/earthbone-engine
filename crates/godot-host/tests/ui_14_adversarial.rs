// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};
use soe_godot_host::race_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn ui_14_preview_includes_hdpt_attached_extra_parts() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut race_data = vec![0; 128];
    race_data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"RACE"), 0x700)
            .sub(T(b"DATA"), race_data)
            .sub(T(b"HNAM"), 0x710_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x710)
            .zsub(T(b"FULL"), "Blind Eye")
            .sub(T(b"DATA"), [5_u8])
            .sub(T(b"PNAM"), 3_u32.to_le_bytes())
            .sub(T(b"HNAM"), 0x711_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x711)
            .zsub(T(b"FULL"), "Right Eye Cover")
            .sub(T(b"DATA"), [5_u8])
            .sub(T(b"PNAM"), 2_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ui14Extras.esp", plugin).unwrap();
    let game = Game::new(order);
    let parts = race_menu::head_parts(&game, id(0x700), true);
    let mut appearance = soe_game::appearance::PlayerAppearance::default();
    appearance.head_parts.insert(3, id(0x710));

    let preview = race_menu::preview_summary(&game, &appearance, &parts);
    assert!(preview.contains("Blind Eye"));
    assert!(preview.contains("Right Eye Cover"));
}

#[test]
fn ui_14_preview_does_not_mutate_player_appearance_before_confirmation() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut race_data = vec![0; 128];
    race_data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    let mut order = LoadOrder::new();
    order.add("Ui14Cancel.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let before = game.world_mut().player_appearance();
    let draft = soe_game::appearance::PlayerAppearance {
        head_parts: [(3, id(0x710))].into_iter().collect(),
        morphs: [("Nose".to_owned(), 0.5)].into_iter().collect(),
    };
    let mut draft = Some(draft);
    let _preview = race_menu::preview_summary(&game, draft.as_ref().unwrap(), &[]);
    race_menu::discard_draft(&mut draft);

    assert!(draft.is_none());
    assert_eq!(game.world_mut().player_appearance(), before);
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert_eq!(game.world_mut().player_appearance(), before);
    assert_eq!(game.player(), player);
}

#[test]
fn ui_14_confirmed_head_parts_and_morphs_survive_game_save_load() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut race_data = vec![0; 128];
    race_data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    plugin.push_top(
        Record::new(T(b"HDPT"), 0x710)
            .sub(T(b"DATA"), [5_u8])
            .sub(T(b"PNAM"), 3_u32.to_le_bytes()),
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
    order.add("Ui14Persist.esp", plugin).unwrap();
    let mut game = Game::new(order);
    {
        let mut world = game.world_mut();
        assert_eq!(world.player_character_traits(), Some((id(0x700), true)));
        world.set_player_head_part(3, id(0x710)).unwrap();
        world.set_player_face_morph("Nose", -1.0).unwrap();
    }
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();

    let appearance = game.world_mut().player_appearance().unwrap();
    assert_eq!(appearance.head_parts.get(&3), Some(&id(0x710)));
    assert_eq!(appearance.morphs.get("Nose"), Some(&-1.0));
}

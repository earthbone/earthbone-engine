// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-18 checks specific-location alias filling and saved identity.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::{Game, quest};

const QUEST: u32 = 0x800;
const TARGET_LOCATION: u32 = 0x801;
const OTHER_LOCATION: u32 = 0x802;
const WRONG_FORM: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn location_alias(flags: u32, location: u32) -> Record {
    quest::build_record(QUEST, "AI18Quest", 0)
        .sub(Tag(*b"ALLS"), 0u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "TargetLocation")
        .sub(Tag(*b"FNAM"), flags.to_le_bytes())
        .sub(Tag(*b"ALFL"), location.to_le_bytes())
        .sub(Tag(*b"ALED"), Vec::new())
}

fn game(quest: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"LCTN"), TARGET_LOCATION));
    plugin.push_top(Record::new(Tag(*b"LCTN"), OTHER_LOCATION));
    plugin.push_top(Record::new(Tag(*b"STAT"), WRONG_FORM));
    plugin.push_top(quest);
    let mut order = LoadOrder::new();
    order.add("AI18.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_18_specific_location_fill_resolves_and_persists_identity() {
    let mut game = game(location_alias(0, TARGET_LOCATION));
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(
        game.world_mut().alias_ref(id(QUEST), 0),
        Some(id(TARGET_LOCATION))
    );

    let saved = game.world_mut().save_state();
    let mut restored = game.world_mut();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.alias_ref(id(QUEST), 0), Some(id(TARGET_LOCATION)));
}

#[test]
fn ai_18_optional_missing_or_wrong_location_stays_empty() {
    for target in [0x900, WRONG_FORM] {
        let mut game = game(location_alias(quest::alias_flags::OPTIONAL, target));
        assert!(game.start_quest(id(QUEST)));
        assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
    }
}

#[test]
fn ai_18_required_missing_or_wrong_location_blocks_start() {
    for target in [0x900, WRONG_FORM] {
        let mut game = game(location_alias(0, target));
        assert!(!game.start_quest(id(QUEST)));
        assert!(!game.world_mut().quest(id(QUEST)).unwrap().running);
        assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
    }
}

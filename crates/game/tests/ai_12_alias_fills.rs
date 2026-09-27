// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-12 checks condition filtering, alias uniqueness and the bounded scan.

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};

const QUEST: u32 = 0x800;
const MATCH_BASE: u32 = 0x801;
const OTHER_BASE: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ctda_is_id(form: u32) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 0];
    bytes.extend_from_slice(&1f32.to_bits().to_le_bytes());
    bytes.extend_from_slice(&72u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&form.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes
}

fn search_alias(id: u32, name: &str, flags: u32, condition: u32) -> Record {
    add_search_alias(
        quest::build_record(QUEST, "AI12Quest", 0),
        id,
        name,
        flags,
        condition,
    )
}

fn add_search_alias(rec: Record, id: u32, name: &str, flags: u32, condition: u32) -> Record {
    rec.sub(Tag(*b"ALST"), id.to_le_bytes())
        .zsub(Tag(*b"ALID"), name)
        .sub(Tag(*b"FNAM"), flags.to_le_bytes())
        .sub(Tag(*b"CTDA"), ctda_is_id(condition))
        .sub(Tag(*b"ALED"), Vec::new())
}

fn build_game(quest_record: Record, references: &[(u32, u32)]) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"STAT"), MATCH_BASE));
    plugin.push_top(Record::new(Tag(*b"STAT"), OTHER_BASE));
    for (object, base) in references {
        plugin.push_top(Record::new(Tag(*b"REFR"), *object).sub(Tag(*b"NAME"), base.to_le_bytes()));
    }
    plugin.push_top(quest_record);
    let mut order = LoadOrder::new();
    order.add("AI12.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_12_fills_only_the_single_reference_matching_authored_conditions() {
    let quest = search_alias(0, "Target", quest::alias_flags::OPTIONAL, MATCH_BASE);
    let mut game = build_game(quest, &[(0x900, OTHER_BASE), (0x901, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), Some(id(0x901)));
}

#[test]
fn ai_12_leaves_ambiguous_and_reused_references_unfilled() {
    let quest = search_alias(0, "Target", quest::alias_flags::OPTIONAL, MATCH_BASE);
    let mut game = build_game(quest, &[(0x900, MATCH_BASE), (0x901, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);

    let quest = search_alias(0, "Required", 0, MATCH_BASE);
    let mut game = build_game(quest, &[(0x900, MATCH_BASE), (0x901, MATCH_BASE)]);
    assert!(!game.start_quest(id(QUEST)));

    let quest = add_search_alias(
        quest::build_record(QUEST, "AI12Quest", 0),
        0,
        "Target",
        quest::alias_flags::OPTIONAL,
        MATCH_BASE,
    )
    .sub(Tag(*b"ALST"), 1u32.to_le_bytes())
    .zsub(Tag(*b"ALID"), "Fixed")
    .sub(Tag(*b"FNAM"), 0u32.to_le_bytes())
    .sub(Tag(*b"ALFR"), 0x900u32.to_le_bytes())
    .sub(Tag(*b"ALED"), Vec::new());
    let mut game = build_game(quest, &[(0x900, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

#[test]
fn ai_12_does_not_fill_from_a_truncated_candidate_set() {
    let quest = search_alias(0, "Target", quest::alias_flags::OPTIONAL, MATCH_BASE);
    let references: Vec<_> = (0..=4096)
        .map(|index| {
            (
                0x1000 + index,
                if index == 0 { MATCH_BASE } else { OTHER_BASE },
            )
        })
        .collect();
    let mut game = build_game(quest, &references);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

#[test]
fn ai_12_allows_reuse_only_when_the_alias_flag_permits_it() {
    let quest = add_search_alias(
        quest::build_record(QUEST, "AI12Quest", 0),
        0,
        "Target",
        quest::alias_flags::OPTIONAL | quest::alias_flags::ALLOW_REUSE_IN_QUEST,
        MATCH_BASE,
    )
    .sub(Tag(*b"ALST"), 1u32.to_le_bytes())
    .zsub(Tag(*b"ALID"), "Fixed")
    .sub(Tag(*b"FNAM"), 0u32.to_le_bytes())
    .sub(Tag(*b"ALFR"), 0x900u32.to_le_bytes())
    .sub(Tag(*b"ALED"), Vec::new());
    let mut game = build_game(quest, &[(0x900, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), Some(id(0x900)));
}

#[test]
fn ai_12_selects_the_unclaimed_reference_when_another_match_is_reserved() {
    let quest = add_search_alias(
        quest::build_record(QUEST, "AI12Quest", 0),
        0,
        "Target",
        0,
        MATCH_BASE,
    )
    .sub(Tag(*b"ALST"), 1u32.to_le_bytes())
    .zsub(Tag(*b"ALID"), "Reserved")
    .sub(
        Tag(*b"FNAM"),
        quest::alias_flags::RESERVES_REFERENCE.to_le_bytes(),
    )
    .sub(Tag(*b"ALFR"), 0x900u32.to_le_bytes())
    .sub(Tag(*b"ALED"), Vec::new());
    let mut game = build_game(quest, &[(0x900, MATCH_BASE), (0x901, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), Some(id(0x901)));
}

#[test]
fn ai_12_failed_quest_start_restores_prior_reference_aliases() {
    let quest = search_alias(0, "Target", 0, MATCH_BASE)
        .sub(Tag(*b"ALLS"), 1u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "MissingLocation")
        .sub(Tag(*b"FNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ALFL"), 0x990u32.to_le_bytes())
        .sub(Tag(*b"ALED"), Vec::new());
    let mut game = build_game(quest, &[(0x900, MATCH_BASE)]);
    assert!(!game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

#[test]
fn ai_12_destroyed_reference_requires_the_authored_permission() {
    for (flags, expected) in [
        (quest::alias_flags::OPTIONAL, None),
        (
            quest::alias_flags::OPTIONAL | quest::alias_flags::ALLOW_DESTROYED,
            Some(id(0x900)),
        ),
    ] {
        let mut game = build_game(
            search_alias(0, "Target", flags, MATCH_BASE),
            &[(0x900, MATCH_BASE)],
        );
        game.world_mut().reference(id(0x900)).unwrap().destroyed = true;
        assert!(game.start_quest(id(QUEST)));
        assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), expected);
    }
}

#[test]
fn ai_12_loaded_area_does_not_mean_a_reference_built_on_demand() {
    let flags = quest::alias_flags::OPTIONAL | quest::alias_flags::IN_LOADED_AREA;
    let mut game = build_game(
        search_alias(0, "Target", flags, MATCH_BASE),
        &[(0x900, MATCH_BASE)],
    );
    assert!(game.world_mut().reference(id(0x900)).is_some());
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

#[test]
fn ai_12_closest_mode_does_not_silently_use_ambiguous_order() {
    let flags = quest::alias_flags::OPTIONAL | quest::alias_flags::CLOSEST;
    let mut game = build_game(
        search_alias(0, "Target", flags, MATCH_BASE),
        &[(0x900, MATCH_BASE)],
    );
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

#[test]
fn ai_12_does_not_treat_create_reference_aliases_as_placed_matches() {
    let quest = quest::build_record(QUEST, "AI12Quest", 0)
        .sub(Tag(*b"ALST"), 0u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Created")
        .sub(Tag(*b"FNAM"), quest::alias_flags::OPTIONAL.to_le_bytes())
        .sub(Tag(*b"CTDA"), ctda_is_id(MATCH_BASE))
        .sub(Tag(*b"ALCO"), MATCH_BASE.to_le_bytes())
        .sub(Tag(*b"ALED"), Vec::new());
    let mut game = build_game(quest, &[(0x900, MATCH_BASE)]);
    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), None);
}

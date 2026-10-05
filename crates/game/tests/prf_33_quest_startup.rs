// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-33 preserves quest startup semantics while reusing authored search candidates.
//! Alias flags and startup stages follow local xEdit and UESP QUST definitions.

use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};

const FIRST_QUEST: u32 = 0x800;
const SECOND_QUEST: u32 = 0x801;
const BASE: u32 = 0x900;
const OTHER_BASE: u32 = 0x901;
const LOCATION: u32 = 0x902;
const FIRST_REF: u32 = 0x1000;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn search_alias(record: Record, alias: u32, alias_flags: u32) -> Record {
    let mut condition = vec![0u8; 32];
    condition[4..8].copy_from_slice(&1f32.to_le_bytes());
    condition[8..10].copy_from_slice(&72u16.to_le_bytes());
    condition[12..16].copy_from_slice(&BASE.to_le_bytes());
    condition[28..32].copy_from_slice(&(-1i32).to_le_bytes());
    record
        .sub(Tag(*b"ALST"), alias.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Searched")
        .sub(Tag(*b"FNAM"), alias_flags.to_le_bytes())
        .sub(Tag(*b"CTDA"), condition)
        .sub(Tag(*b"ALED"), Vec::new())
}

fn searched_quest(object: u32, alias_flags: u32) -> Record {
    search_alias(
        quest::build_record(object, "SearchedQuest", 0),
        0,
        alias_flags,
    )
}

fn order(quests: Vec<Record>, references: usize) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OTHER_BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag(*b"LCTN"), LOCATION));
    for index in (0..references).rev() {
        let base = if index + 1 == references {
            BASE
        } else {
            OTHER_BASE
        };
        let tag = if index % 2 == 0 {
            Tag(*b"REFR")
        } else {
            Tag(*b"ACHR")
        };
        plugin.push_top(
            Record::new(tag, FIRST_REF + index as u32).sub(Tag(*b"NAME"), base.to_le_bytes()),
        );
    }
    for record in quests {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("PRF33.esm", plugin).unwrap();
    order
}

#[test]
fn prf_33_large_orders_preserve_forced_location_aliases_and_startup_stages() {
    let quest = quest::with_stage(
        quest::build_record(FIRST_QUEST, "ForcedQuest", 1),
        10,
        2,
        0,
        "Start",
    );
    let quest = quest::with_alias(quest, 0, "Forced", Some(FIRST_REF))
        .sub(Tag(*b"ALLS"), 1u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Location")
        .sub(Tag(*b"FNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ALFL"), LOCATION.to_le_bytes())
        .sub(Tag(*b"ALED"), Vec::new());
    let complete = quest::with_stage(
        quest::build_record(SECOND_QUEST, "CompletingQuest", 1),
        20,
        2,
        1,
        "Complete",
    );
    let mut game = Game::new(order(vec![quest, complete], 4097));
    assert_eq!(game.start().quests_started, 2);
    let mut world = game.world_mut();
    assert_eq!(world.alias_ref(id(FIRST_QUEST), 0), Some(id(FIRST_REF)));
    assert_eq!(world.alias_ref(id(FIRST_QUEST), 1), Some(id(LOCATION)));
    let first = world.quest(id(FIRST_QUEST)).unwrap();
    assert!(first.running && first.done.contains(&10) && first.highest == 10);
    let second = world.quest(id(SECOND_QUEST)).unwrap();
    assert!(
        second.completed && !second.running && second.done.contains(&20) && second.highest == 20
    );
}

#[test]
fn prf_33_reused_candidates_read_live_eligibility_on_each_start() {
    let mut game = Game::new(order(
        vec![
            searched_quest(FIRST_QUEST, 0),
            searched_quest(SECOND_QUEST, 0),
        ],
        1,
    ));
    assert!(game.start_quest(id(FIRST_QUEST)));
    assert_eq!(
        game.world_mut().alias_ref(id(FIRST_QUEST), 0),
        Some(id(FIRST_REF))
    );
    game.world_mut().reference(id(FIRST_REF)).unwrap().enabled = false;
    assert!(
        !game.start_quest(id(SECOND_QUEST)),
        "cached identities must not cache enabled state"
    );
    assert_eq!(game.world_mut().alias_ref(id(SECOND_QUEST), 0), None);
    game.world_mut().reference(id(FIRST_REF)).unwrap().enabled = true;
    assert!(game.start_quest(id(SECOND_QUEST)));
    assert_eq!(
        game.world_mut().alias_ref(id(SECOND_QUEST), 0),
        Some(id(FIRST_REF))
    );
    assert!(game.world_mut().stop_quest(id(SECOND_QUEST)));
    game.world_mut().reference(id(FIRST_REF)).unwrap().destroyed = true;
    assert!(
        !game.start_quest(id(SECOND_QUEST)),
        "restart must reevaluate destroyed state"
    );
    assert_eq!(
        game.world_mut().alias_ref(id(SECOND_QUEST), 0),
        Some(id(FIRST_REF)),
        "failure restores the prior alias"
    );
    {
        let mut world = game.world_mut();
        let reference = world.reference(id(FIRST_REF)).unwrap();
        reference.destroyed = false;
        reference.base = Some(id(OTHER_BASE));
    }
    assert!(
        !game.start_quest(id(SECOND_QUEST)),
        "cached candidates must reevaluate the authored GetIsID condition"
    );
}

#[test]
fn prf_33_searches_the_complete_mixed_candidate_set_at_the_limit() {
    let mut game = Game::new(order(
        vec![
            searched_quest(FIRST_QUEST, 0),
            searched_quest(SECOND_QUEST, 0),
        ],
        4096,
    ));
    for quest in [FIRST_QUEST, SECOND_QUEST] {
        assert!(
            game.start_quest(id(quest)),
            "4096 candidates remain searchable"
        );
        assert_eq!(
            game.world_mut().alias_ref(id(quest), 0),
            Some(id(FIRST_REF + 4095))
        );
    }
}

#[test]
fn prf_33_overflow_never_fills_from_a_partial_candidate_set() {
    let forced = quest::with_alias(
        searched_quest(SECOND_QUEST, 0),
        1,
        "Forced",
        Some(FIRST_REF),
    );
    let mut game = Game::new(order(
        vec![
            searched_quest(FIRST_QUEST, quest::alias_flags::OPTIONAL),
            forced,
        ],
        4097,
    ));
    assert!(game.start_quest(id(FIRST_QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(FIRST_QUEST), 0), None);
    assert!(
        !game.start_quest(id(SECOND_QUEST)),
        "required search fails even when the overflow result is cached"
    );
    assert_eq!(game.world_mut().alias_ref(id(SECOND_QUEST), 0), None);
    assert_eq!(
        game.world_mut().alias_ref(id(SECOND_QUEST), 1),
        Some(id(FIRST_REF))
    );
    assert!(!game.world().quest(id(SECOND_QUEST)).unwrap().running);
}

#[test]
fn prf_33_candidates_use_winning_overrides_and_stay_scoped_to_the_order() {
    let quests = vec![
        searched_quest(FIRST_QUEST, 0),
        searched_quest(SECOND_QUEST, 0),
    ];
    let mut load_order = order(quests.clone(), 2);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["PRF33.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"REFR"), FIRST_REF).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    patch.push_top(
        Record::new(Tag(*b"ACHR"), FIRST_REF + 1).sub(Tag(*b"NAME"), OTHER_BASE.to_le_bytes()),
    );
    load_order.add("Patch.esp", patch).unwrap();
    let shared = Arc::new(load_order);
    let mut patched = Game::from_shared_order(Arc::clone(&shared));
    for quest in [FIRST_QUEST, SECOND_QUEST] {
        assert!(patched.start_quest(id(quest)));
        assert_eq!(
            patched.world_mut().alias_ref(id(quest), 0),
            Some(id(FIRST_REF))
        );
    }
    let mut unpatched = Game::new(order(quests, 2));
    assert!(unpatched.start_quest(id(FIRST_QUEST)));
    assert_eq!(
        unpatched.world_mut().alias_ref(id(FIRST_QUEST), 0),
        Some(id(FIRST_REF + 1))
    );
    let mut fresh = Game::from_shared_order(shared);
    fresh.world_mut().reference(id(FIRST_REF)).unwrap().enabled = false;
    assert!(
        !fresh.start_quest(id(FIRST_QUEST)),
        "worlds sharing an order still own separate live eligibility"
    );
}

#[test]
fn prf_33_installed_startup_quests_finish_within_release_budget() {
    let Some(data) = crate::common::installed::data_dir() else {
        return;
    };
    let mut game = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let scripts = game.load_scripts_from_data(&data);
    assert!(
        scripts.scripts_loaded > 14000,
        "load the installed startup script cohort: {scripts:?}"
    );
    assert!(scripts.scripts_failed.is_empty(), "{scripts:?}");
    let bound = game.bind();
    assert!(
        bound.scripts_attached > 18000,
        "bind the installed startup scripts: {bound:?}"
    );
    let quests: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"QUST"))
        .map(|(id, _)| id)
        .collect();
    let budget = Duration::from_millis(1500);
    let begin = Instant::now();
    let (mut attempted, mut started) = (0usize, 0usize);
    let mut per_quest = Vec::new();
    for id in quests {
        if game
            .world_mut()
            .quest_state(id)
            .is_some_and(|quest| quest.def.starts_enabled())
        {
            attempted += 1;
            let begin = Instant::now();
            started += usize::from(game.start_quest(id));
            per_quest.push(begin.elapsed());
        }
    }
    let elapsed = begin.elapsed();
    per_quest.sort_unstable();
    eprintln!("PRF-33 startup: {attempted} attempted, {started} started, total {elapsed:?}, median {:?}, max {:?}, release={}", per_quest.get(per_quest.len() / 2), per_quest.last(), !cfg!(debug_assertions));
    assert_eq!(
        started, 421,
        "retain the lead's installed startup quest count"
    );
    if !cfg!(debug_assertions) {
        assert!(
            elapsed <= budget,
            "startup quests took {elapsed:?}, release budget {budget:?}"
        );
    }
    assert!(game.run_until_idle(64), "startup scripts must settle");
}

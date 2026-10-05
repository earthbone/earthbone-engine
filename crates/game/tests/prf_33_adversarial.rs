// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! PRF-33 adversarial checks for cached quest alias candidates.

use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::{
    plugin::{flags, FileHeader},
    GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{quest, Game};

const QUEST_A: u32 = 0x800;
const QUEST_B: u32 = 0x801;
const BASE: u32 = 0x900;
const REF: u32 = 0x1000;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn searched_quest(object: u32, alias_flags: u32) -> Record {
    let mut condition = vec![0u8; 32];
    condition[4..8].copy_from_slice(&1f32.to_le_bytes());
    condition[8..10].copy_from_slice(&72u16.to_le_bytes());
    condition[12..16].copy_from_slice(&BASE.to_le_bytes());
    condition[28..32].copy_from_slice(&(-1i32).to_le_bytes());
    quest::build_record(object, "Searched", 0)
        .sub(Tag(*b"ALST"), 0u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Target")
        .sub(Tag(*b"FNAM"), alias_flags.to_le_bytes())
        .sub(Tag(*b"CTDA"), condition)
        .sub(Tag(*b"ALED"), Vec::new())
}

fn order(quests: Vec<Record>, actor: bool) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    let tag = if actor { Tag(*b"ACHR") } else { Tag(*b"REFR") };
    plugin.push_top(Record::new(tag, REF).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    for quest in quests {
        plugin.push_top(quest);
    }
    let mut order = LoadOrder::new();
    order.add("PRF33.esm", plugin).unwrap();
    order
}

#[test]
fn prf_33_cached_actor_identity_uses_live_dead_flag_per_quest() {
    let mut game = Game::new(order(
        vec![
            searched_quest(QUEST_A, quest::alias_flags::OPTIONAL),
            searched_quest(
                QUEST_B,
                quest::alias_flags::OPTIONAL | quest::alias_flags::ALLOW_DEAD,
            ),
        ],
        true,
    ));
    game.world_mut().actor(id(REF)).unwrap().dead = true;
    assert!(game.start_quest(id(QUEST_A)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST_A), 0), None);
    assert!(game.start_quest(id(QUEST_B)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST_B), 0), Some(id(REF)));
}

#[test]
fn prf_33_deleted_winning_reference_cannot_be_selected_from_cached_candidates() {
    let mut load_order = order(
        vec![searched_quest(QUEST_A, quest::alias_flags::OPTIONAL)],
        false,
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["PRF33.esm".into()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(Tag(*b"REFR"), REF)
            .with_flags(flags::DELETED)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes()),
    );
    load_order.add("Patch.esp", patch).unwrap();
    let mut game = Game::from_shared_order(Arc::new(load_order));
    assert!(game.start_quest(id(QUEST_A)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST_A), 0), None);
}

#[test]
fn prf_33_overflow_marker_rejects_required_search_after_optional_start() {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    for index in 0..4097u32 {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), REF + index).sub(Tag(*b"NAME"), BASE.to_le_bytes()),
        );
    }
    plugin.push_top(searched_quest(QUEST_A, quest::alias_flags::OPTIONAL));
    plugin.push_top(searched_quest(QUEST_B, 0));
    let mut load_order = LoadOrder::new();
    load_order.add("PRF33.esm", plugin).unwrap();
    let mut game = Game::new(load_order);
    assert!(game.start_quest(id(QUEST_A)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST_A), 0), None);
    assert!(!game.start_quest(id(QUEST_B)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST_B), 0), None);
    assert!(!game.world().quest(id(QUEST_B)).unwrap().running);
}

#[test]
fn prf_33_installed_startup_quests_meet_release_budget() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
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
        let plugin = Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap();
        order.add(name, plugin).unwrap();
    }
    let mut game = Game::from_shared_order(Arc::new(order));
    let scripts = game.load_scripts_from_data(&data);
    assert!(scripts.scripts_loaded > 14_000, "{scripts:?}");
    assert!(scripts.scripts_failed.is_empty(), "{scripts:?}");
    let bound = game.bind();
    assert!(bound.scripts_attached > 18_000, "{bound:?}");
    let quest_forms: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"QUST"))
        .map(|(form, _)| form)
        .collect();
    let startup: Vec<_> = quest_forms
        .into_iter()
        .filter(|form| {
            game.world_mut()
                .quest_state(*form)
                .is_some_and(|quest| quest.def.starts_enabled())
        })
        .collect();
    let begin = Instant::now();
    let started = startup
        .iter()
        .filter(|form| game.start_quest(**form))
        .count();
    let elapsed = begin.elapsed();
    assert_eq!(started, 421);
    if !cfg!(debug_assertions) {
        assert!(elapsed <= Duration::from_millis(1500), "{elapsed:?}");
    }
    assert!(game.run_until_idle(64));
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};
use std::time::{Duration, Instant};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn prf_33_closest_loaded_reference_alias_selects_nearest_matching_reference() {
    const CELL: u32 = 1;
    const BASE: u32 = 2;
    const NEAR: u32 = 3;
    const FAR: u32 = 4;
    const QUEST: u32 = 5;

    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    for (form, x) in [(NEAR, 20.0_f32), (FAR, 200.0_f32)] {
        let mut placement = vec![0_u8; 24];
        placement[..4].copy_from_slice(&x.to_le_bytes());
        let mut reference = Record::new(Tag(*b"REFR"), form)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"DATA"), placement);
        reference.placement.cell = Some(CELL);
        plugin.push_top(reference);
    }
    let mut condition = vec![0_u8; 32];
    condition[4..8].copy_from_slice(&1_f32.to_le_bytes());
    condition[8..10].copy_from_slice(&72_u16.to_le_bytes());
    condition[12..16].copy_from_slice(&BASE.to_le_bytes());
    condition[28..32].copy_from_slice(&(-1_i32).to_le_bytes());
    plugin.push_top(
        quest::build_record(QUEST, "ClosestAlias", 0)
            .sub(Tag(*b"ALST"), 0_u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "Target")
            .sub(
                Tag(*b"FNAM"),
                (quest::alias_flags::CLOSEST | quest::alias_flags::IN_LOADED_AREA)
                    .to_le_bytes(),
            )
            .sub(Tag(*b"CTDA"), condition)
            .sub(Tag(*b"ALED"), Vec::new()),
    );
    let mut order = LoadOrder::new();
    order.add("PRF33.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_cell(id(CELL));

    assert!(game.start_quest(id(QUEST)));
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), Some(id(NEAR)));
}

#[test]
fn prf_33_installed_startup_quests_meet_release_budget() {
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
    assert!(scripts.scripts_loaded > 14_000, "{scripts:?}");
    assert!(scripts.scripts_failed.is_empty(), "{scripts:?}");
    let bound = game.bind();
    assert!(bound.scripts_attached > 18_000, "{bound:?}");
    let startup: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"QUST"))
        .map(|(form, _)| form)
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

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-09: SCEN phase progression waits for ending actions and survives SOEW.
//! SCEN fields follow local xEdit and UESP definitions. The CK scene pages are
//! absent from the local corpus, so action completion is supplied by the
//! owning world API in these runtime tests.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene() -> Record {
    Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Wait")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 10u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Second action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 11u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes())
}

fn fixture_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(scene());
    plugin.push_top(soe_game::quest::build_record(0x810, "SceneQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    Game::new(order)
}

/// dlg_09_scene_waits_for_required_actions_and_persists_its_phase
#[test]
fn dlg_09_scene_waits_for_required_actions_and_persists_its_phase() {
    let game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    let mut world = game.world_mut();
    assert!(world.start_scene(id(0x800), &mut allocate));
    assert_eq!(world.scene_phase(id(0x800)), Some(0));
    assert!(!world.advance_scene_phase(id(0x800), &mut allocate));
    assert!(!world.complete_scene_action(id(0x800), 2));
    assert!(world.complete_scene_action(id(0x800), 0));
    assert!(!world.advance_scene_phase(id(0x800), &mut allocate));
    assert!(world.complete_scene_action(id(0x800), 1));
    assert!(world.advance_scene_phase(id(0x800), &mut allocate));
    assert_eq!(world.scene_phase(id(0x800)), Some(1));
    drop(world);

    let save = game.save_state().unwrap();
    let mut restored = fixture_game();
    restored.load_state(&save).unwrap();
    assert_eq!(restored.world().scene_phase(id(0x800)), Some(1));
    assert!(restored.world().scenes.contains(&id(0x800)));
}

#[test]
fn dlg_09_scene_rejects_an_action_without_required_phase_bounds() {
    for missing in [Tag(*b"SNAM"), Tag(*b"ENAM")] {
        let mut record = scene();
        let at = record
            .subrecords
            .iter()
            .position(|subrecord| subrecord.tag == missing)
            .unwrap();
        record.subrecords.remove(at);
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(record);
        let mut order = LoadOrder::new();
        order.add("Scene.esp", plugin).unwrap();
        let game = Game::new(order);
        let mut allocate = |_: &str, _: &str| ObjectId(1);
        assert!(
            !game.world_mut().start_scene(id(0x800), &mut allocate),
            "SCEN actions require both phase bounds"
        );
    }
}

fn fragment_game() -> Game {
    fragment_game_with(scene())
}

fn fragment_game_with(record: Record) -> Game {
    use soe_game::vmad::{InfoFragment, SceneFragments, ScenePhaseFragment, ScriptEntry, Vmad};
    use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallStatic, Return};

    let callback = |function: &str| InfoFragment {
        marker: 1,
        script: "SceneCallbacks".into(),
        function: function.into(),
    };
    let vmad = Vmad {
        scripts: vec![
            ScriptEntry::new("SceneDecoy"),
            ScriptEntry::new("SceneCallbacks"),
        ],
        scene_fragments: Some(SceneFragments {
            version: 2,
            file: "SceneCallbacks".into(),
            begin: Some(callback("Begin")),
            end: Some(callback("End")),
            phases: [
                (1, 0, "Start0"),
                (2, 0, "End0"),
                (1, 1, "Start1"),
                (2, 1, "End1"),
                (1, 2, "Start2"),
                (2, 2, "End2"),
                (1, 3, "Start3"),
                (2, 3, "End3"),
            ]
            .into_iter()
            .map(|(flag, phase_index, function)| ScenePhaseFragment {
                flag,
                phase_index,
                marker: 1,
                script: "SceneCallbacks".into(),
                function: function.into(),
            })
            .collect(),
        }),
        ..Vmad::default()
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record.sub(Tag(*b"VMAD"), vmad.write(Tag(*b"SCEN"))));
    plugin.push_top(soe_game::quest::build_record(0x810, "SceneQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    let mut game = Game::new(order);
    for script in ["SceneDecoy", "SceneCallbacks"] {
        let mut builder = ScriptBuilder::new(script, "Scene");
        for function in [
            "Begin", "End", "Start0", "End0", "Start1", "End1", "Start2", "End2", "Start3", "End3",
        ] {
            builder = builder.function(
                FunctionBuilder::new(function, "None")
                    .local("unused", "None")
                    .ins(
                        CallStatic,
                        vec![
                            s("Debug"),
                            s("Trace"),
                            ident("unused"),
                            s(&format!("{script}.{function}")),
                            i(0),
                        ],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            );
        }
        game.load_script(&builder.bytes()).unwrap();
    }
    game.bind();
    game
}

#[test]
fn ply_03_scene_skips_false_start_phases_without_actions_or_fragments() {
    let mut condition = vec![0; 32];
    condition[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    condition[8..10].copy_from_slice(&59u16.to_le_bytes());
    condition[12..16].copy_from_slice(&0x810u32.to_le_bytes());
    condition[16..20].copy_from_slice(&10u32.to_le_bytes());
    let mut record = Record::new(Tag(*b"SCEN"), 0x800);
    for phase in 0..4 {
        record = record
            .sub(Tag(*b"HNAM"), [])
            .zsub(Tag(*b"NAM0"), &format!("Phase {phase}"));
        if phase == 1 || phase == 2 {
            record = record.sub(Tag(*b"CTDA"), condition.clone());
        }
        record = record.sub(Tag(*b"NEXT"), []).sub(Tag(*b"HNAM"), []);
    }
    record = record
        .sub(Tag(*b"ANAM"), 2u16.to_le_bytes())
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 10u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 1u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 2u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 3600.0f32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes());
    let mut game = fragment_game_with(record);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    let report = soe_game::play::run(
        &mut game,
        &soe_game::play::Route::parse("play-scene Scene.esp:800 3 1").unwrap(),
    );
    assert!(report.passed(), "{}", report.format());
    assert_eq!(game.world().scene_phase(id(0x800)), Some(3));
    assert_eq!(
        callbacks(&game),
        [
            "SceneCallbacks.Begin",
            "SceneCallbacks.Start0",
            "SceneCallbacks.End0",
            "SceneCallbacks.Start3"
        ]
    );
    assert_eq!(
        game.world().scene_action_slot_complete(id(0x800), 0),
        Some(false)
    );
}

#[test]
fn ply_03_scene_spanning_timer_keeps_its_deadline_on_phase_entry() {
    let mut record = Record::new(Tag(*b"SCEN"), 0x800);
    for phase in 0..3 {
        record = record
            .sub(Tag(*b"HNAM"), [])
            .zsub(Tag(*b"NAM0"), &format!("Phase {phase}"))
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"HNAM"), []);
    }
    record = record
        .sub(Tag(*b"ANAM"), 2u16.to_le_bytes())
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 10u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 1u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0.03125f32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes());
    let mut game = fragment_game_with(record);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    game.tick(0.02);
    let report = soe_game::play::run(
        &mut game,
        &soe_game::play::Route::parse("play-scene Scene.esp:800 2 3").unwrap(),
    );
    assert!(report.passed(), "{}", report.format());
    assert_eq!(game.world().scene_phase(id(0x800)), Some(2));
}

fn callbacks(game: &Game) -> Vec<&str> {
    game.vm
        .log
        .entries
        .iter()
        .map(|entry| entry.message.as_str())
        .filter(|message| {
            message.starts_with("SceneCallbacks.") || message.starts_with("SceneDecoy.")
        })
        .collect()
}

#[test]
fn ply_03_finish_scene_waits_for_actions_and_dispatches_the_end_fragment_once() {
    let record = Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 2u16.to_le_bytes())
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 10u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0.03125f32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes());
    let mut game = fragment_game_with(record);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    let route = soe_game::play::Route::parse("finish-scene Scene.esp:800 1").unwrap();
    let report = soe_game::play::run(&mut game, &route);
    assert!(!report.passed());
    assert_eq!(game.world().scene_phase(id(0x800)), Some(0));
    assert_eq!(
        callbacks(&game),
        ["SceneCallbacks.Begin", "SceneCallbacks.Start0"]
    );
    let route = soe_game::play::Route::parse("finish-scene Scene.esp:800 3").unwrap();
    let report = soe_game::play::run(&mut game, &route);
    assert!(report.passed(), "{}", report.format());
    assert_eq!(game.world().scene_phase(id(0x800)), None);
    assert_eq!(
        callbacks(&game),
        [
            "SceneCallbacks.Begin",
            "SceneCallbacks.Start0",
            "SceneCallbacks.End0",
            "SceneCallbacks.End",
        ]
    );
    let report = soe_game::play::run(&mut game, &route);
    assert!(!report.passed());
    assert_eq!(
        callbacks(&game)
            .iter()
            .filter(|name| **name == "SceneCallbacks.End")
            .count(),
        1
    );
    assert!(soe_game::play::Route::parse("finish-scene Scene.esp:800 36001").is_err());
    assert!(soe_game::play::Route::parse("finish-scene Scene.esp:800").is_err());
}

#[test]
fn dlg_09_scene_boundaries_call_named_fragments_once_in_phase_order() {
    let mut game = fragment_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    assert!(!game.world_mut().start_scene(id(0x800), &mut allocate));
    game.tick(0.0);
    assert_eq!(
        callbacks(&game),
        ["SceneCallbacks.Begin", "SceneCallbacks.Start0"]
    );
    assert!(!game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    game.tick(0.0);
    assert_eq!(
        callbacks(&game),
        ["SceneCallbacks.Begin", "SceneCallbacks.Start0"]
    );
    assert!(game.world_mut().complete_scene_action(id(0x800), 0));
    assert!(game.world_mut().complete_scene_action(id(0x800), 1));
    assert!(game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    game.tick(0.0);
    assert_eq!(
        callbacks(&game),
        [
            "SceneCallbacks.Begin",
            "SceneCallbacks.Start0",
            "SceneCallbacks.End0",
            "SceneCallbacks.Start1"
        ]
    );
    assert!(game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    assert!(!game.world_mut().stop_scene(id(0x800), &mut allocate));
    game.tick(0.0);
    assert_eq!(
        callbacks(&game),
        [
            "SceneCallbacks.Begin",
            "SceneCallbacks.Start0",
            "SceneCallbacks.End0",
            "SceneCallbacks.Start1",
            "SceneCallbacks.End1",
            "SceneCallbacks.End"
        ]
    );
}

#[test]
fn dlg_09_scene_restore_does_not_replay_begin_or_phase_start_fragments() {
    let mut game = fragment_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    game.tick(0.0);
    assert!(game.world_mut().complete_scene_action(id(0x800), 0));
    assert!(game.world_mut().complete_scene_action(id(0x800), 1));
    assert!(game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    game.tick(0.0);
    let save = game.save_state().unwrap();
    let mut restored = fragment_game();
    restored.load_state(&save).unwrap();
    restored.tick(0.0);
    assert_eq!(callbacks(&restored), callbacks(&game));
    assert!(restored.world_mut().stop_scene(id(0x800), &mut allocate));
    restored.tick(0.0);
    assert_eq!(
        callbacks(&restored),
        [
            "SceneCallbacks.Begin",
            "SceneCallbacks.Start0",
            "SceneCallbacks.End0",
            "SceneCallbacks.Start1",
            "SceneCallbacks.End",
        ]
    );
}

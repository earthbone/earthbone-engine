// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-44: Scene.IsActionComplete queries the tracked action slot by SCEN INAM.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::records::gameplay::{Scene, SceneActionData};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::{ObjectId, Value};

const SCENE: u32 = 0x800;
static PROBES: AtomicUsize = AtomicUsize::new(0);
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene() -> Record {
    Record::new(Tag(*b"SCEN"), SCENE)
        .zsub(Tag(*b"EDID"), "TestScene")
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Wait")
        .sub(
            Tag(*b"VMAD"),
            Vmad {
                scripts: vec![ScriptEntry::new("SceneScript")],
                ..Default::default()
            }
            .write(Tag(*b"SCEN")),
        )
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "First action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 101u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Second action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 202u32.to_le_bytes())
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
    let mut game = Game::new(order);
    game.load_script(&scene_script()).unwrap();
    game.start();
    game
}

fn malformed_fixture_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), SCENE)
            .zsub(Tag(*b"EDID"), "TestScene")
            .sub(Tag(*b"HNAM"), [])
            .sub(Tag(*b"ANAM"), [0]),
    );
    let mut order = LoadOrder::new();
    order.add("MalformedScene.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(&scene_script()).unwrap();
    game.start();
    game
}

fn scene_script() -> Vec<u8> {
    ScriptBuilder::new("SceneScript", "Scene")
        .function(FunctionBuilder::new("OnBegin", "None").build())
        .bytes()
}

fn call_action_complete(game: &mut Game, scene_id: GlobalFormId, action_id: i32) -> bool {
    let probe = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr44Probe{probe}");
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("t", "Form")
        .param("actionID", "Int")
        .local("r", "Var")
        .ins(
            CallMethod,
            vec![
                s("IsActionComplete"),
                ident("t"),
                ident("r"),
                ident("actionID"),
            ],
        )
        .ins(Return, vec![ident("r")])
        .build();
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(function)
        .bytes();
    game.load_script(&bytes).unwrap();
    let target = Value::Object(game.object_of_form(scene_id));
    let thread = game
        .vm
        .call_global(&script, "Run", &[target, Value::Int(action_id)])
        .unwrap();
    game.run_until_idle(64);
    game.vm
        .thread_result(thread)
        .unwrap_or(Value::Bool(false))
        .as_bool()
}

#[test]
fn scr_44_action_id_reports_the_completed_authored_slot() {
    let mut game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(SCENE), &mut allocate));
    assert!(!call_action_complete(&mut game, id(SCENE), 101));
    assert!(!call_action_complete(&mut game, id(SCENE), 202));
    assert!(game.world_mut().complete_scene_action(id(SCENE), 0));
    assert!(game.world().scene_action_complete(id(SCENE), 101));
    assert!(
        call_action_complete(&mut game, id(SCENE), 101),
        "{:?}",
        game.traces()
    );
    assert!(!call_action_complete(&mut game, id(SCENE), 202));
    assert!(game.world_mut().complete_scene_action(id(SCENE), 1));
    assert!(call_action_complete(&mut game, id(SCENE), 202));
    assert!(game.world_mut().stop_scene(id(SCENE), &mut allocate));
    assert!(!call_action_complete(&mut game, id(SCENE), 101));
}

#[test]
fn scr_44_invalid_or_unknown_action_ids_return_false() {
    let mut game = fixture_game();
    assert!(game.vm.natives.has_method("scene", "isactioncomplete"));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(SCENE), &mut allocate));
    assert!(!call_action_complete(&mut game, id(SCENE), -1));
    assert!(!call_action_complete(&mut game, id(SCENE), 999));
    assert!(
        game.traces().iter().all(|trace| !trace.contains("error")),
        "{:?}",
        game.traces()
    );
}

#[test]
fn scr_44_malformed_scene_does_not_report_an_action_complete() {
    let mut game = malformed_fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(!game.world_mut().start_scene(id(SCENE), &mut allocate));
    assert!(!call_action_complete(&mut game, id(SCENE), 101));
}

#[test]
fn scr_44_vanilla_overlapping_action_ids_remain_scene_scoped() {
    let Some(mut game) = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping the vanilla scene action check");
        return;
    };
    let worshippers = game.form("T03WorshippersScene").unwrap();
    let dg_scene_id = game.form("DGScene04").unwrap();
    let (action_id, worshippers_slot) = {
        let world = game.world();
        let worshippers_record = world.record(worshippers).unwrap();
        let dg_record = world.record(dg_scene_id).unwrap();
        let worshippers_scene = Scene::from_record(worshippers_record).unwrap();
        let dg_scene = Scene::from_record(dg_record).unwrap();
        let (slot, action_id) = worshippers_scene
            .actions
            .iter()
            .enumerate()
            .find_map(|(slot, action)| {
                let action_id = action.index?;
                (action.start_phase == Some(0)
                    && !matches!(action.data, SceneActionData::Timer { .. })
                    && dg_scene
                        .actions
                        .iter()
                        .any(|action| action.index == Some(action_id)))
                .then_some((slot as u32, action_id))
            })
            .expect("vanilla scenes share a non-timer action number active in phase zero");
        (action_id, slot)
    };

    game.load_script(&scene_script()).unwrap();
    {
        let world = game.shared_world();
        let mut allocate = |name: &str, class: &str| game.vm.create_object_of(name, class);
        assert!(world.borrow_mut().start_scene(worshippers, &mut allocate));
        assert!(world.borrow_mut().start_scene(dg_scene_id, &mut allocate));
    }
    assert!(!call_action_complete(
        &mut game,
        worshippers,
        action_id as i32
    ));
    assert!(game
        .world_mut()
        .complete_scene_action(worshippers, worshippers_slot));
    assert!(call_action_complete(
        &mut game,
        worshippers,
        action_id as i32
    ));
    assert!(!call_action_complete(
        &mut game,
        dg_scene_id,
        action_id as i32
    ));
}

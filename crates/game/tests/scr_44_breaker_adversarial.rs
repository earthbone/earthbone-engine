// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{ObjectId, Value};

const SCENE: u32 = 0x800;
const ACTION_ID: i32 = 55;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene_record() -> Record {
    Record::new(Tag(*b"SCEN"), SCENE)
        .zsub(Tag(*b"EDID"), "BreakerScene")
        .sub(
            Tag(*b"VMAD"),
            Vmad {
                scripts: vec![ScriptEntry::new("SceneScript")],
                ..Default::default()
            }
            .write(Tag(*b"SCEN")),
        )
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Start")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Wait")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Sparse authored action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), (ACTION_ID as u32).to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
}

fn malformed_scene_record() -> Record {
    Record::new(Tag(*b"SCEN"), 0x802)
        .zsub(Tag(*b"EDID"), "MalformedScene")
        .sub(
            Tag(*b"VMAD"),
            Vmad {
                scripts: vec![ScriptEntry::new("SceneScript")],
                ..Default::default()
            }
            .write(Tag(*b"SCEN")),
        )
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), [0])
}

fn fixture_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(scene_record());
    plugin.push_top(malformed_scene_record());
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(&scene_script()).unwrap();
    game.load_script(&script()).unwrap();
    game.start();
    game
}

#[test]
fn scr_44_malformed_scene_returns_false_without_raising_a_native_error() {
    let mut game = fixture_game();
    assert!(!query(&mut game, id(0x802), ACTION_ID));
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}

fn scene_script() -> Vec<u8> {
    ScriptBuilder::new("SceneScript", "Scene")
        .function(FunctionBuilder::new("OnBegin", "None").build())
        .bytes()
}

fn script() -> Vec<u8> {
    ScriptBuilder::new("Scr44Breaker", "Form")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("target", "Form")
                .param("action", "Int")
                .local("result", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("IsActionComplete"),
                        ident("target"),
                        ident("result"),
                        ident("action"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes()
}

fn query(game: &mut Game, scene: GlobalFormId, action: i32) -> bool {
    let target = Value::Object(game.object_of_form(scene));
    let thread = game
        .vm
        .call_global("Scr44Breaker", "Run", &[target, Value::Int(action)])
        .unwrap();
    game.run_until_idle(32);
    game.vm.thread_result(thread).unwrap().as_bool()
}

#[test]
fn scr_44_completed_authored_id_survives_save_load_and_repeated_queries() {
    let mut game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(SCENE), &mut allocate));
    assert!(game.world_mut().complete_scene_action(id(SCENE), 0));

    assert!(
        query(&mut game, id(SCENE), ACTION_ID),
        "{:?}",
        game.traces()
    );
    assert!(query(&mut game, id(SCENE), ACTION_ID));
    let save = game.save_state().unwrap();

    let mut restored = fixture_game();
    restored.load_state(&save).unwrap();
    assert!(query(&mut restored, id(SCENE), ACTION_ID));
    assert!(!query(&mut restored, id(SCENE), 0));
    assert!(restored
        .traces()
        .iter()
        .all(|trace| !trace.contains("error")));
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    ObjectId,
    Opcode::{CallMethod, Return},
    Value,
};
fn id(n: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: n }
}
fn game() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    let scene = Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Start")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Wait")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Only action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 55u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x800u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), []);
    p.push_top(scene);
    let mut o = LoadOrder::new();
    o.add("Scene.esm", p).unwrap();
    Game::new(o)
}
fn call(g: &mut Game, action: i32) -> Value {
    let script = "Scr44Adv";
    let bytes = ScriptBuilder::new(script, "Scene")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("target", "Scene")
                .param("action", "Int")
                .local("r", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("IsActionComplete"),
                        ident("target"),
                        ident("r"),
                        ident("action"),
                    ],
                )
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    g.load_script(&bytes).unwrap();
    let target = Value::Object(g.object_of_form(id(0x800)));
    let t =
        g.vm.call_global(script, "Run", &[target, Value::Int(action)])
            .unwrap();
    g.run_until_idle(32);
    g.vm.thread_result(t).unwrap()
}
#[test]
fn scr_44_inactive_scene_never_reports_actions_complete() {
    let mut g = game();
    for action in [-1, 0, i32::MAX] {
        assert_eq!(call(&mut g, action), Value::Bool(false));
    }
}
#[test]
fn scr_44_completion_requires_authored_action_id_not_slot_number() {
    let g = game();
    let mut alloc = |_: &str, _: &str| ObjectId(1);
    assert!(g.world_mut().start_scene(id(0x800), &mut alloc));
    assert!(g.world_mut().complete_scene_action(id(0x800), 0));
    assert!(!g.world().scene_action_complete(id(0x800), 0));
    assert!(g.world().scene_action_complete(id(0x800), 55));
}

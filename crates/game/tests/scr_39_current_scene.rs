// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-39: resolve an active scene from its quest alias participants.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::ObjectId;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const SCENE: u32 = 0x800;
const QUEST: u32 = 0x801;
const ACTOR_REF: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"QUST"), QUEST)
            .sub(Tag(*b"DNAM"), vec![0; 12])
            .sub(Tag(*b"ALST"), 0u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "Actor")
            .sub(Tag(*b"FNAM"), 2u32.to_le_bytes())
            .sub(Tag(*b"ALFR"), ACTOR_REF.to_le_bytes())
            .sub(Tag(*b"ALED"), []),
    );
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), SCENE)
            .sub(Tag(*b"HNAM"), [])
            .zsub(Tag(*b"NAM0"), "Phase")
            .sub(Tag(*b"ALID"), 0i32.to_le_bytes())
            .sub(Tag(*b"PNAM"), QUEST.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), ACTOR_REF)
            .sub(Tag(*b"NAME"), 0x803u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    Game::new(order)
}

fn call_current_scene(game: &mut Game, reference: GlobalFormId) -> Value {
    let script = ScriptBuilder::new("Scr39Probe", "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("reference", "ObjectReference")
                .local("scene", "Var")
                .ins(
                    CallMethod,
                    vec![s("GetCurrentScene"), ident("reference"), ident("scene")],
                )
                .ins(Return, vec![ident("scene")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let target = game.object_of_form(reference);
    let thread = game
        .vm
        .call_global("Scr39Probe", "Run", &[Value::Object(target)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_39_inactive_scene_is_not_current() {
    let game = game();
    assert_eq!(game.world_mut().current_scene(id(ACTOR_REF)), None);
}

#[test]
fn scr_39_active_scene_matches_resolved_actor_alias() {
    let mut game = game();
    game.object_of_form(id(SCENE));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    {
        let mut world = game.world_mut();
        assert!(world.start_scene(id(SCENE), &mut allocate));
        assert_eq!(world.current_scene(id(ACTOR_REF)), Some(id(SCENE)));
        assert_eq!(world.current_scene(id(ACTOR_REF + 1)), None);
    }
    let scene = call_current_scene(&mut game, id(ACTOR_REF));
    assert_eq!(scene.as_object(), Some(game.object_of_form(id(SCENE))));
}

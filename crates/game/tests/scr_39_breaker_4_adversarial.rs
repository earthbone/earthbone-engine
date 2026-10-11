// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-39 checks for deterministic current-scene selection.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::ObjectId;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_game::Game;

const FIRST_SCENE: u32 = 0x800;
const SECOND_SCENE: u32 = 0x810;
const QUEST: u32 = 0x801;
const ACTOR_REF: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene_record(form: u32) -> Record {
    Record::new(Tag(*b"SCEN"), form)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Phase")
        .sub(Tag(*b"ALID"), 0i32.to_le_bytes())
        .sub(Tag(*b"PNAM"), QUEST.to_le_bytes())
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
    plugin.push_top(scene_record(SECOND_SCENE));
    plugin.push_top(scene_record(FIRST_SCENE));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), ACTOR_REF)
            .sub(Tag(*b"NAME"), 0x803u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Scr39SceneOrder.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn current_scene_chooses_lowest_form_id_when_active_scenes_share_an_actor() {
    let mut game = game();
    game.object_of_form(id(FIRST_SCENE));
    game.object_of_form(id(SECOND_SCENE));
    let mut next_object = 1;
    let mut allocate = |_: &str, _: &str| {
        let object = ObjectId(next_object);
        next_object += 1;
        object
    };
    {
        let mut world = game.world_mut();
        assert!(world.start_scene(id(SECOND_SCENE), &mut allocate));
        assert!(world.start_scene(id(FIRST_SCENE), &mut allocate));
    }

    let script = ScriptBuilder::new("Scr39SceneOrderProbe", "Form")
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
    let actor = Value::Object(game.object_of_form(id(ACTOR_REF)));
    let thread = game
        .vm
        .call_global(
            "Scr39SceneOrderProbe",
            "Run",
            &[actor],
        )
        .unwrap();
    game.run_until_idle(64);

    assert_eq!(
        game.vm.thread_result(thread).and_then(|value| value.as_object()),
        Some(game.object_of_form(id(FIRST_SCENE)))
    );
}

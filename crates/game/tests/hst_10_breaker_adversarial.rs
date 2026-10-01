// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[path = "common/installed.rs"]
mod installed;

use soe_esm::Tag;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn call_play_animation(game: &mut Game, target: Value) -> Value {
    let reference_api = ScriptBuilder::new("ObjectReference", "Form")
        .function(
            FunctionBuilder::new("PlayAnimation", "Bool")
                .param("animation", "String")
                .native()
                .build(),
        )
        .bytes();
    game.load_script(&reference_api)
        .expect("load ObjectReference API");
    let script = "Hst10ActorPlayAnimationProbe";
    let bytes = ScriptBuilder::new(script, "Form")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("target", "ObjectReference")
                .local("result", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("PlayAnimation"),
                        ident("target"),
                        ident("result"),
                        s("IdleForceDefaultState"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).expect("load probe script");
    let thread = game
        .vm
        .call_global(script, "Run", &[target])
        .expect("start probe");
    game.run_until_idle(32);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn play_animation_rejects_actor_targets() {
    let Some(mut game) = installed::game(&["Skyrim.esm"]) else {
        eprintln!("SKYRIM_DATA is unset, skipping installed actor animation contract");
        return;
    };
    let actors: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag::new(b"ACHR"))
        .map(|(id, _)| id)
        .collect();
    let actor = actors
        .into_iter()
        .find(|id| game.world_mut().actor(*id).is_some())
        .expect("installed placed actor");
    let target = Value::Object(game.object_of_form(actor));

    assert_eq!(call_play_animation(&mut game, target), Value::Bool(false));
    assert!(game.world().animation_requests.is_empty());
}

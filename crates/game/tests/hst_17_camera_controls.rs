// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_global(game: &mut Game, name: &str, arg_type: &str, arg: Value) {
    let script = format!("Hst17Probe{}", game.vm.time().to_bits());
    let fun = FunctionBuilder::new("Run", "None")
        .global()
        .param("value", arg_type)
        .ins(
            CallStatic,
            vec![s("Game"), s(name), ident("unused"), ident("value")],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    game.load_script(&ScriptBuilder::new(&script, "").function(fun).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[arg]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn hst_17_game_natives_set_camera_target_geometry_and_sitting_offset() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.world_mut().player();
    let actor = Value::Object(game.object_of_form(player));
    call_global(&mut game, "SetCameraTarget", "Actor", actor);
    call_global(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );
    call_global(&mut game, "SetSittingRotation", "Float", Value::Float(37.5));

    let world = game.world();
    assert_eq!(world.camera_controls.target, Some(player));
    assert!(!world.camera_controls.first_person_geometry);
    assert_eq!(world.camera_controls.sitting_rotation_degrees, 37.5);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_godot_host::camera::CameraHostState;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_physics::Vec3;

fn call(game: &mut Game, name: &str, kind: &str, arg: Value) {
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("value", kind)
        .ins(
            CallStatic,
            vec![s("Game"), s(name), ident("unused"), ident("value")],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    let script = format!("Hst17Adversarial{}", game.vm.time().to_bits());
    game.load_script(&ScriptBuilder::new(&script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[arg]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn hst_17_camera_target_switch_is_applied_to_host_anchor() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.world_mut().player();
    let object = game.object_of_form(player);
    call(&mut game, "SetCameraTarget", "Actor", Value::Object(object));

    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    assert_eq!(state.target(), Some(player));
    let pose = (player, Vec3::new(40.0, -20.0, 7.0), 176.0);
    assert_eq!(
        state.anchor(Vec3::ZERO, 128.0, Some(pose)),
        (pose.1, pose.2)
    );
}

#[test]
fn hst_17_sitting_rotation_accepts_negative_offset_and_geometry_can_be_restored() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );
    call(
        &mut game,
        "SetSittingRotation",
        "Float",
        Value::Float(-45.0),
    );
    assert!(!game.world().camera_controls.first_person_geometry);
    assert_eq!(game.world().camera_controls.sitting_rotation_degrees, -45.0);
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(true),
    );
    assert!(game.world().camera_controls.first_person_geometry);
}

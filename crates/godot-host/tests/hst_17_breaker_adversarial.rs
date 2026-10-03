// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_godot_host::camera::{CameraHostState, CameraMode, CameraRig};
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
    let script = format!("Hst17Breaker{}", game.vm.time().to_bits());
    game.load_script(&ScriptBuilder::new(&script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[arg]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn hst_17_target_anchor_tracks_selected_actor_pose() {
    let mut game = Game::new(LoadOrder::new());
    let target = game.world_mut().player();
    let object = game.object_of_form(target);
    call(&mut game, "SetCameraTarget", "Actor", Value::Object(object));

    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    let moved_pose = (target, Vec3::new(-25.0, 90.0, 12.0), 176.0);
    assert_eq!(state.target(), Some(target));
    assert_eq!(
        state.anchor(Vec3::ZERO, 128.0, Some(moved_pose)),
        (moved_pose.1, moved_pose.2)
    );
}

#[test]
fn hst_17_first_person_geometry_native_last_call_wins() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(true),
    );
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );

    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    assert!(!state.first_person_geometry_visible());
}

#[test]
fn hst_17_sitting_offset_changes_camera_yaw_without_turning_movement() {
    let mut game = Game::new(LoadOrder::new());
    call(&mut game, "SetSittingRotation", "Float", Value::Float(90.0));

    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    rig.set_facing(0.0, 0.0);
    let movement = rig.forward_flat();
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    let eye = rig.eye(Vec3::ZERO, 128.0, None);

    assert_eq!(movement, Vec3::new(0.0, 1.0, 0.0));
    assert!((eye.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert_eq!(rig.forward_flat(), movement);
}

#[test]
fn hst_17_zero_sitting_rotation_clears_prior_camera_offset() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "SetSittingRotation",
        "Float",
        Value::Float(-135.0),
    );
    call(&mut game, "SetSittingRotation", "Float", Value::Float(0.0));

    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    let mut rig = CameraRig::default();
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    assert_eq!(state.sitting_rotation_degrees(), 0.0);
    assert_eq!(rig.eye(Vec3::ZERO, 128.0, None).yaw, 0.0);
}

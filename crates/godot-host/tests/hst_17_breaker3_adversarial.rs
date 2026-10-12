// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder};
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
    let script = format!("Hst17Adversarial{}", game.vm.time().to_bits());
    game.load_script(&ScriptBuilder::new(&script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[arg]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn host_state(game: &Game) -> CameraHostState {
    let mut state = CameraHostState::default();
    state.apply(game.world().camera_controls);
    state
}

#[test]
fn hst_17_native_target_pose_and_rotation_reach_camera_eye() {
    let mut game = Game::new(LoadOrder::new());
    let target = game.world_mut().player();
    let object = game.object_of_form(target);
    call(&mut game, "SetCameraTarget", "Actor", Value::Object(object));
    call(&mut game, "SetSittingRotation", "Float", Value::Float(45.0));

    let state = host_state(&game);
    let feet = Vec3::new(120.0, -80.0, 24.0);
    let (anchor, height) = state.anchor(Vec3::ZERO, 128.0, Some((target, feet, 176.0)));
    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    let eye = rig.eye(anchor, height, None);

    assert_eq!(state.target(), Some(target));
    assert_eq!((anchor, height), (feet, 176.0));
    assert!((eye.yaw - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    assert_ne!(eye.position, rig.eye(Vec3::ZERO, 128.0, None).position);
}

#[test]
fn hst_17_invalid_camera_target_does_not_discard_actor_target() {
    let mut game = Game::new(LoadOrder::new());
    let target = game.world_mut().player();
    let object = game.object_of_form(target);
    call(&mut game, "SetCameraTarget", "Actor", Value::Object(object));
    call(&mut game, "SetCameraTarget", "Actor", Value::None);

    let state = host_state(&game);
    let target_pose = (target, Vec3::new(3.0, 4.0, 5.0), 160.0);
    assert_eq!(state.target(), Some(target));
    assert_eq!(
        state.anchor(Vec3::ZERO, 128.0, Some(target_pose)),
        (target_pose.1, target_pose.2)
    );
}

#[test]
fn hst_17_first_person_geometry_follows_native_in_both_camera_modes() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );
    let hidden = host_state(&game);
    assert!(!hidden.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(hidden.rendered_first_person_geometry_visible(CameraMode::Third));

    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(true),
    );
    let shown = host_state(&game);
    assert!(shown.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(shown.rendered_first_person_geometry_visible(CameraMode::Third));
}

#[test]
fn hst_17_camera_uses_player_pose_when_target_pose_id_differs() {
    let mut game = Game::new(LoadOrder::new());
    let target = game.world_mut().player();
    let object = game.object_of_form(target);
    call(&mut game, "SetCameraTarget", "Actor", Value::Object(object));

    let state = host_state(&game);
    let player_pose = (Vec3::new(-10.0, 15.0, 20.0), 128.0);
    let unrelated_pose = (
        GlobalFormId {
            file: target.file,
            object: target.object + 1,
        },
        Vec3::new(90.0, 80.0, 70.0),
        180.0,
    );
    assert_eq!(
        state.anchor(player_pose.0, player_pose.1, Some(unrelated_pose)),
        player_pose
    );
}

#[test]
fn hst_17_rotation_replacement_and_zero_reach_host_camera() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "SetSittingRotation",
        "Float",
        Value::Float(-90.0),
    );
    call(&mut game, "SetSittingRotation", "Float", Value::Float(0.0));

    let state = host_state(&game);
    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    assert_eq!(state.sitting_rotation_degrees(), 0.0);
    assert_eq!(rig.eye(Vec3::ZERO, 128.0, None).yaw, 0.0);
}

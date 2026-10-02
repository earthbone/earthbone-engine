// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{save_generations::GenerationStore, world::HostRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};
pub use soe_godot_host::units;
use std::time::Duration;

#[path = "../src/camera.rs"]
mod camera_impl;
#[path = "../src/door_fade.rs"]
mod fade_impl;

fn invoke(game: &mut Game, function: &str, args: &[Value]) {
    let script = format!("Scr28Host{function}");
    let mut body = FunctionBuilder::new("Run", "None").global();
    let mut operands = vec![s("Game"), s(function), ident("unused")];
    for (index, _) in args.iter().enumerate() {
        let name = format!("arg{index}");
        body = body.param(&name, "Var");
        operands.push(ident(&name));
    }
    game.load_script(
        &ScriptBuilder::new(&script, "")
            .function(body.ins(CallStatic, operands).build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(16));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_28_white_fade_keeps_its_color_when_door_fade_has_equal_opacity() {
    let mut door = fade_impl::DoorFade::default();
    assert!(door.record_preparation(Duration::from_millis(51)));
    door.advance(Duration::from_millis(75));
    let mut game = fade_impl::GameFade::default();
    assert!(game.request(true, false, 0.0, 1.0));
    game.advance(0.5);

    let presentation = fade_impl::fade_overlay_presentation(&door, &game);
    assert_eq!(presentation.rgba, [1.0, 1.0, 1.0, 0.5]);
    assert!(presentation.visible);
}

#[test]
fn scr_28_zero_duration_fades_complete_after_the_requested_delay() {
    let mut fade = fade_impl::GameFade::default();
    assert!(fade.request(true, true, 0.25, 0.0));
    assert_eq!(fade.advance(0.24), 0.0);
    assert_eq!(fade.advance(0.01), 1.0);
    assert!(fade.request(false, false, 0.0, 0.0));
    assert_eq!(fade.advance(0.0), 0.0);
}

#[test]
fn scr_28_new_fade_replaces_in_progress_fade_and_resets_elapsed_time() {
    let mut fade = fade_impl::GameFade::default();
    assert!(fade.request(true, true, 0.0, 2.0));
    assert_eq!(fade.advance(1.0), 0.5);
    assert!(fade.request(true, false, 0.5, 1.0));
    assert_eq!(fade.advance(0.25), 0.0);
    assert_eq!(fade.advance(0.25), 0.0);
    assert_eq!(fade.advance(0.5), 0.5);
    assert!(!fade.is_black());
}

#[test]
fn scr_28_camera_requests_set_the_requested_rig_mode_repeatedly() {
    let mut rig = camera_impl::CameraRig::default()
        .with_mode(camera_impl::CameraMode::Third);
    camera_impl::apply_game_camera_request(&mut rig, true);
    assert_eq!(rig.mode, camera_impl::CameraMode::First);
    camera_impl::apply_game_camera_request(&mut rig, false);
    assert_eq!(rig.mode, camera_impl::CameraMode::Third);
    camera_impl::apply_game_camera_request(&mut rig, true);
    assert_eq!(rig.mode, camera_impl::CameraMode::First);
}

#[test]
fn scr_28_manual_and_automatic_requests_write_recoverable_separate_generations() {
    let root = tempfile::tempdir().unwrap();
    let manual_dir = root.path().join("manual");
    let automatic_dir = root.path().join("automatic");
    std::fs::create_dir_all(&manual_dir).unwrap();
    std::fs::create_dir_all(&automatic_dir).unwrap();
    let manual_store = GenerationStore::new(&manual_dir, 3).unwrap();
    let automatic_store = GenerationStore::new(&automatic_dir, 3).unwrap();
    let mut game = Game::new(LoadOrder::new());
    invoke(&mut game, "RequestSave", &[]);
    invoke(&mut game, "RequestAutoSave", &[]);

    let requests = game.world_mut().take_host_requests();
    assert_eq!(
        requests,
        [
            HostRequest::Save { automatic: false },
            HostRequest::Save { automatic: true }
        ]
    );
    let manual = game.write_generation(&manual_store).unwrap();
    let automatic = game.write_generation(&automatic_store).unwrap();
    assert_eq!((manual.generation, automatic.generation), (1, 1));
    assert_ne!(manual.path.parent(), automatic.path.parent());

    let mut restored_manual = Game::new(LoadOrder::new());
    let mut restored_automatic = Game::new(LoadOrder::new());
    assert_eq!(
        restored_manual
            .recover_generation(&manual_store)
            .unwrap()
            .generation,
        1
    );
    assert_eq!(
        restored_automatic
            .recover_generation(&automatic_store)
            .unwrap()
            .generation,
        1
    );
}

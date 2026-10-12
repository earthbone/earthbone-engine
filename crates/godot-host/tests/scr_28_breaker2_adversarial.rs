// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::HostRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

#[path = "../src/door_fade.rs"]
#[allow(dead_code)]
mod fade_impl;

fn invoke(game: &mut Game, function: &str, args: &[Value]) {
    let script = format!("Scr28Breaker{function}{}", game.vm.time().to_bits());
    let mut body = FunctionBuilder::new("Run", "None").global();
    let mut operands = vec![s("Game"), s(function), ident("unused")];
    for (index, _) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        body = body.param(&parameter, "Var");
        operands.push(ident(&parameter));
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
fn scr_28_each_non_finite_or_negative_fade_timing_rejects_without_a_request() {
    let mut game = Game::new(LoadOrder::new());
    for (delay, duration) in [
        (f32::NAN, 1.0),
        (f32::INFINITY, 1.0),
        (0.0, f32::NEG_INFINITY),
        (-f32::EPSILON, 1.0),
        (0.0, -f32::EPSILON),
    ] {
        invoke(
            &mut game,
            "FadeOutGame",
            &[
                Value::Bool(true),
                Value::Bool(false),
                Value::Float(delay),
                Value::Float(duration),
            ],
        );
        assert!(game.world().host_requests.is_empty());
    }
    assert_eq!(
        game.traces()
            .iter()
            .filter(|trace| trace.contains("Game.FadeOutGame"))
            .count(),
        5
    );
}

#[test]
fn scr_28_invalid_fade_does_not_discard_or_reorder_a_valid_pending_request() {
    let mut game = Game::new(LoadOrder::new());
    invoke(
        &mut game,
        "FadeOutGame",
        &[
            Value::Bool(false),
            Value::Bool(true),
            Value::Float(0.25),
            Value::Float(0.75),
        ],
    );
    invoke(
        &mut game,
        "FadeOutGame",
        &[
            Value::Bool(true),
            Value::Bool(false),
            Value::Float(0.0),
            Value::Float(f32::NAN),
        ],
    );
    assert_eq!(
        game.world_mut().take_host_requests(),
        [HostRequest::FadeOutGame {
            fading_out: false,
            black: true,
            seconds_before_fade: 0.25,
            fade_duration: 0.75,
        }]
    );
}

#[test]
fn scr_28_repeated_saves_and_camera_requests_keep_each_call_in_order() {
    let mut game = Game::new(LoadOrder::new());
    for name in [
        "RequestSave",
        "RequestSave",
        "RequestAutoSave",
        "ForceThirdPerson",
        "ForceFirstPerson",
        "ForceThirdPerson",
    ] {
        invoke(&mut game, name, &[]);
    }
    assert_eq!(
        game.world_mut().take_host_requests(),
        [
            HostRequest::Save { automatic: false },
            HostRequest::Save { automatic: false },
            HostRequest::Save { automatic: true },
            HostRequest::CameraThirdPerson,
            HostRequest::CameraFirstPerson,
            HostRequest::CameraThirdPerson,
        ]
    );
    assert!(game.world().host_requests.is_empty());
}

#[test]
fn scr_28_fade_time_boundaries_and_bad_frame_deltas_do_not_advance_transition() {
    let mut fade = fade_impl::GameFade::default();
    assert!(fade.request(true, false, 0.25, 0.5));
    assert_eq!(fade.advance(f32::NAN), 0.0);
    assert_eq!(fade.advance(f32::INFINITY), 0.0);
    assert_eq!(fade.advance(-1.0), 0.0);
    assert_eq!(fade.advance(0.25), 0.0);
    assert_eq!(fade.advance(0.25), 0.5);
    assert_eq!(fade.advance(0.25), 1.0);

    assert!(fade.request(false, true, 0.25, 0.5));
    assert_eq!(fade.advance(0.25), 1.0);
    assert_eq!(fade.advance(0.25), 0.5);
    assert_eq!(fade.advance(0.25), 0.0);
}

#[test]
fn scr_28_zero_duration_fade_in_clears_a_previously_visible_game_fade() {
    let mut fade = fade_impl::GameFade::default();
    assert!(fade.request(true, true, 0.0, 0.0));
    assert_eq!(fade.advance(0.0), 1.0);
    assert!(fade.request(false, false, 0.0, 0.0));
    assert_eq!(fade.advance(0.0), 0.0);
    let overlay = fade_impl::fade_overlay_presentation(&fade_impl::DoorFade::default(), &fade);
    assert_eq!(overlay.rgba, [1.0, 1.0, 1.0, 0.0]);
    assert!(!overlay.visible);
}

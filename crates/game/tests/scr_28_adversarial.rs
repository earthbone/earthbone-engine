// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{Game, world::HostRequest};
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use soe_papyrus::{Opcode::*, Value};

fn call(game: &mut Game, name: &str, args: &[Value]) {
    let script = format!("Scr28{name}{}", game.vm.time().to_bits());
    let mut f = FunctionBuilder::new("Run", "None").global();
    let mut op = vec![s("Game"), s(name), ident("unused")];
    for (i, _) in args.iter().enumerate() {
        let p = format!("a{i}");
        f = f.param(&p, "Var");
        op.push(ident(&p));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(f.ins(CallStatic, op).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let t = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(t), Some(Value::None));
}

#[test]
fn scr_28_save_and_camera_requests_preserve_order_and_distinguish_save_kind() {
    let mut game = Game::new(LoadOrder::new());
    for name in [
        "RequestSave",
        "RequestAutoSave",
        "ForceFirstPerson",
        "ForceThirdPerson",
    ] {
        call(&mut game, name, &[]);
    }
    assert_eq!(
        game.world_mut().take_host_requests(),
        [
            HostRequest::Save { automatic: false },
            HostRequest::Save { automatic: true },
            HostRequest::CameraFirstPerson,
            HostRequest::CameraThirdPerson
        ]
    );
    assert!(game.world().host_requests.is_empty());
}

#[test]
fn scr_28_fade_forwards_both_flags_and_rejects_invalid_timing_atomically() {
    let mut game = Game::new(LoadOrder::new());
    call(
        &mut game,
        "FadeOutGame",
        &[
            Value::Bool(false),
            Value::Bool(true),
            Value::Float(1.25),
            Value::Float(0.75),
        ],
    );
    assert_eq!(
        game.world_mut().take_host_requests(),
        [HostRequest::FadeOutGame {
            fading_out: false,
            black: true,
            seconds_before_fade: 1.25,
            fade_duration: 0.75
        }]
    );
    call(
        &mut game,
        "FadeOutGame",
        &[
            Value::Bool(true),
            Value::Bool(false),
            Value::Float(0.0),
            Value::Float(0.0),
        ],
    );
    assert_eq!(
        game.world_mut().take_host_requests(),
        [HostRequest::FadeOutGame {
            fading_out: true,
            black: false,
            seconds_before_fade: 0.0,
            fade_duration: 0.0
        }]
    );
    call(
        &mut game,
        "FadeOutGame",
        &[
            Value::Bool(true),
            Value::Bool(true),
            Value::Float(-1.0),
            Value::Float(0.5),
        ],
    );
    assert!(game.world().host_requests.is_empty());
    assert!(
        game.traces()
            .iter()
            .any(|trace| trace.contains("Game.FadeOutGame"))
    );
}

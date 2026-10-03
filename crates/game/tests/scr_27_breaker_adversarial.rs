// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn shake(game: &mut Game, left: f32, right: f32, duration: f32) {
    let script = format!("Scr27Shake{}", game.vm.time().to_bits());
    let fun = FunctionBuilder::new("Run", "None")
        .global()
        .param("left", "Float")
        .param("right", "Float")
        .param("duration", "Float")
        .ins(
            CallStatic,
            vec![
                s("Game"),
                s("ShakeController"),
                ident("unused"),
                ident("left"),
                ident("right"),
                ident("duration"),
            ],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    game.load_script(&ScriptBuilder::new(&script, "").function(fun).bytes())
        .unwrap();
    let thread = game
        .vm
        .call_global(
            &script,
            "Run",
            &[
                Value::Float(left),
                Value::Float(right),
                Value::Float(duration),
            ],
        )
        .unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn set_sun_modifier(game: &mut Game, modifier: Value) {
    let script = format!("Scr27SetSun{}", game.vm.time().to_bits());
    let fun = FunctionBuilder::new("Run", "None")
        .global()
        .param("modifier", "Form")
        .ins(
            CallStatic,
            vec![
                s("Game"),
                s("SetSunGazeImageSpaceModifier"),
                ident("unused"),
                ident("modifier"),
            ],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    game.load_script(&ScriptBuilder::new(&script, "").function(fun).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[modifier]).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_27_shake_clamps_both_strengths_and_rejects_each_non_finite_argument() {
    let mut game = Game::new(LoadOrder::new());
    shake(&mut game, -2.0, 3.0, 0.0);
    assert!(matches!(
        game.world().host_requests.as_slice(),
        [soe_game::HostRequest::ControllerShake {
            left_strength: 0.0,
            right_strength: 1.0,
            duration: 0.0,
        }]
    ));

    for (left, right, duration) in [
        (f32::NAN, 0.5, 1.0),
        (0.5, f32::INFINITY, 1.0),
        (0.5, 0.5, f32::INFINITY),
    ] {
        shake(&mut game, left, right, duration);
    }
    assert_eq!(game.world().host_requests.len(), 1);
    assert_eq!(game.world().native_errors.len(), 3);
}

#[test]
fn scr_27_invalid_sun_modifier_is_reported_without_changing_player_state() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.world_mut().player();
    let player_object = game.object_of_form(player);
    set_sun_modifier(&mut game, Value::Object(player_object));
    assert_eq!(game.world().native_errors.len(), 1);
    assert!(game.world().image_space_layers().is_empty());

    set_sun_modifier(&mut game, Value::None);
    assert_eq!(game.world().native_errors.len(), 1);
    assert!(game.world().image_space_layers().is_empty());
}

#[test]
fn scr_27_zero_bounty_servetime_is_repeatable_without_clock_or_jail_mutation() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.world_mut().player();
    {
        let mut world = game.world_mut();
        world.actor(player).unwrap().days_in_jail = 4;
    }
    let before = game.world().clock.clone();
    for _ in 0..2 {
        let script = format!("Scr27ServeTime{}", game.vm.time().to_bits());
        let fun = FunctionBuilder::new("Run", "None")
            .global()
            .ins(CallStatic, vec![s("Game"), s("ServeTime"), ident("unused")])
            .ins(Return, vec![soe_papyrus::asm::none()])
            .build();
        game.load_script(&ScriptBuilder::new(&script, "").function(fun).bytes())
            .unwrap();
        let thread = game.vm.call_global(&script, "Run", &[]).unwrap();
        assert!(game.run_until_idle(32));
        assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    }
    assert_eq!(game.world().clock, before);
    assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail, 4);
}

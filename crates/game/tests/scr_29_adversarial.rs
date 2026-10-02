// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! SCR-29 Game native request ordering and argument boundary cases.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{BinkOptions, GameUiRequest};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"AVIF"), 0x800)
            .sub(Tag::new(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                Tag::new(b"AVSK"),
                [2.0_f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag::new(b"GMST"), 0x801)
            .sub(Tag::new(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(Tag::new(b"DATA"), 2.0_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr29Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, name: &str, args: &[Value]) {
    let class = format!("Scr29Breaker{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Game"), s(name), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(
            &parameter,
            match value {
                Value::Bool(_) => "Bool",
                Value::Float(_) => "Float",
                Value::Int(_) => "Int",
                Value::Str(_) => "String",
                _ => "Var",
            },
        );
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&class, "Form")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&class, "Run", args).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread).unwrap(), Value::None);
}

#[test]
fn scr_29_repeated_hud_toggles_and_ui_requests_keep_order_and_arguments() {
    let mut game = game();
    call(&mut game, "SetHudCartMode", &[Value::Bool(true)]);
    call(&mut game, "ShowRaceMenu", &[]);
    call(&mut game, "TriggerScreenBlood", &[Value::Int(7)]);
    call(
        &mut game,
        "PlayBink",
        &[
            Value::string("Video/Intro.bik"),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
        ],
    );
    call(&mut game, "SetHudCartMode", &[Value::Bool(false)]);
    assert!(!game.world().hud_cart_mode);
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        vec![
            GameUiRequest::ShowRaceMenu,
            GameUiRequest::TriggerScreenBlood(7),
            GameUiRequest::PlayBink {
                file_name: "Video/Intro.bik".into(),
                options: BinkOptions {
                    interruptible: false,
                    mute_audio: true,
                    mute_music: false,
                    letterbox: true
                }
            },
        ]
    );
    assert!(game.world_mut().take_game_ui_requests().is_empty());
}

#[test]
fn scr_29_invalid_bink_native_reports_error_and_does_not_enqueue_request() {
    let mut game = game();
    call(
        &mut game,
        "PlayBink",
        &[
            Value::string("  "),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
        ],
    );
    assert_eq!(game.world().native_errors.len(), 1);
    assert!(game.world().game_ui_requests.is_empty());
}

#[test]
fn scr_29_advance_skill_zero_is_noop_and_negative_input_is_rejected() {
    let mut game = game();
    let player = game.player();
    let before = game.world_mut().base_actor_value(player, "OneHanded");
    call(
        &mut game,
        "AdvanceSkill",
        &[Value::string("OneHanded"), Value::Float(0.0)],
    );
    call(
        &mut game,
        "AdvanceSkill",
        &[Value::string("OneHanded"), Value::Float(-1.0)],
    );
    assert_eq!(
        game.world_mut().base_actor_value(player, "OneHanded"),
        before
    );
    assert_eq!(game.world().native_errors.len(), 1);
}

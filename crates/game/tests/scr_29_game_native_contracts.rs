// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! SCR-29 Game native arguments, diagnostics and host requests.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::{BinkOptions, GameUiError, GameUiRequest};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn call_global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr29Probe{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Game"), s(name), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        let ty = match value {
            Value::Bool(_) => "Bool",
            Value::Float(_) => "Float",
            Value::Int(_) => "Int",
            Value::Str(_) => "String",
            _ => "Var",
        };
        function = function.param(&parameter, ty);
        operands.push(ident(&parameter));
    }
    function = function
        .ins(CallStatic, operands)
        .ins(Return, vec![ident("result")]);
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"AVIF"), 0x800)
            .sub(Tag::new(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                Tag::new(b"AVSK"),
                [2.0f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag::new(b"GMST"), 0x801)
            .sub(Tag::new(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(Tag::new(b"DATA"), 2.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("SCR29.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_29_set_hud_cart_mode_updates_shared_world_state() {
    let mut game = game();
    assert_eq!(
        call_global(&mut game, "SetHudCartMode", &[Value::Bool(true)]),
        Value::None
    );
    assert!(game.world().hud_cart_mode);
    call_global(&mut game, "SetHudCartMode", &[Value::Bool(false)]);
    assert!(!game.world().hud_cart_mode);
}

#[test]
fn scr_29_show_race_menu_queues_host_request() {
    let mut game = game();
    assert_eq!(call_global(&mut game, "ShowRaceMenu", &[]), Value::None);
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        vec![GameUiRequest::ShowRaceMenu]
    );
}

#[test]
fn scr_29_trigger_screen_blood_preserves_integer_argument() {
    let mut game = game();
    assert_eq!(
        call_global(&mut game, "TriggerScreenBlood", &[Value::Int(-3)]),
        Value::None
    );
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        vec![GameUiRequest::TriggerScreenBlood(-3)]
    );
}

#[test]
fn scr_29_play_bink_preserves_all_options_and_rejects_empty_name() {
    let mut game = game();
    let args = [
        Value::string("video/intro.bik"),
        Value::Bool(true),
        Value::Bool(false),
        Value::Bool(true),
        Value::Bool(false),
    ];
    assert_eq!(call_global(&mut game, "PlayBink", &args), Value::None);
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        vec![GameUiRequest::PlayBink {
            file_name: "video/intro.bik".into(),
            options: BinkOptions {
                interruptible: true,
                mute_audio: false,
                mute_music: true,
                letterbox: false,
            },
        }]
    );
    assert_eq!(
        game.world_mut().play_bink(
            " \t",
            BinkOptions {
                interruptible: true,
                mute_audio: true,
                mute_music: true,
                letterbox: true,
            }
        ),
        Err(GameUiError::EmptyVideoName)
    );
    assert!(game.world().game_ui_requests.is_empty());
}

#[test]
fn scr_29_advance_skill_reports_invalid_skill_and_preserves_arguments() {
    let mut game = game();
    assert_eq!(
        call_global(
            &mut game,
            "AdvanceSkill",
            &[Value::string("NotASkill"), Value::Float(2.5)]
        ),
        Value::None
    );
    assert_eq!(
        game.world().native_errors,
        vec![("Game.AdvanceSkill".into(), "unknown skill NotASkill".into())]
    );
    assert_eq!(
        call_global(
            &mut game,
            "AdvanceSkill",
            &[Value::string("OneHanded"), Value::Float(2.5)]
        ),
        Value::None
    );
    let player = game.player();
    assert!(game.world_mut().base_actor_value(player, "OneHanded") > 0.0);
    assert!(
        game.world()
            .skill_experience(player, "OneHanded")
            .is_some_and(|experience| experience > 0.0)
    );
    assert_eq!(game.world().native_errors.len(), 1);
    assert!(
        game.world_mut()
            .advance_skill(player, "Health", 2.5)
            .is_err()
    );
}

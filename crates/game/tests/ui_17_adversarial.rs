// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn game() -> Game {
    Game::new(LoadOrder::new())
}

#[test]
fn ui_17_adversarial_precache_and_clear_keep_their_order_across_repeated_calls() {
    let mut game = game();
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "None")
        .ins(
            soe_papyrus::Opcode::CallStatic,
            vec![s("Game"), s("PrecacheCharGen"), ident("result")],
        )
        .ins(
            soe_papyrus::Opcode::CallStatic,
            vec![s("Game"), s("PrecacheCharGen"), ident("result")],
        )
        .ins(
            soe_papyrus::Opcode::CallStatic,
            vec![s("Game"), s("PrecacheCharGenClear"), ident("result")],
        )
        .ins(soe_papyrus::Opcode::Return, vec![])
        .build();
    game.load_script(
        &ScriptBuilder::new("ChargenProbe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm.call_global("ChargenProbe", "Run", &[]).unwrap();
    assert!(game.run_until_idle(16));
    assert_eq!(
        game.world().menu_requests,
        [
            soe_game::world::MenuRequest::PrecacheCharGen,
            soe_game::world::MenuRequest::PrecacheCharGen,
            soe_game::world::MenuRequest::PrecacheCharGenClear,
        ]
    );
}

#[test]
fn ui_17_precache_request_runs_before_race_menu_request() {
    let mut game = game();
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "None")
        .ins(
            soe_papyrus::Opcode::CallStatic,
            vec![s("Game"), s("PrecacheCharGen"), ident("result")],
        )
        .ins(
            soe_papyrus::Opcode::CallStatic,
            vec![s("Game"), s("ShowRaceMenu"), ident("result")],
        )
        .ins(soe_papyrus::Opcode::Return, vec![])
        .build();
    game.load_script(
        &ScriptBuilder::new("ChargenMenuProbe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm.call_global("ChargenMenuProbe", "Run", &[]).unwrap();
    assert!(game.run_until_idle(16));
    assert_eq!(
        game.world().menu_requests,
        [
            soe_game::world::MenuRequest::PrecacheCharGen,
            soe_game::world::MenuRequest::Race,
        ]
    );
    assert_eq!(
        game.world().game_ui_requests,
        [soe_game::world::GameUiRequest::ShowRaceMenu]
    );
}

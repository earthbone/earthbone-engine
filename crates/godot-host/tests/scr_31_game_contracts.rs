// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-31 checks the menu globals described in CK Game_Script.wikitext and
//! SKSE's shipped Game.psc wrappers.

use soe_esm::LoadOrder;
use soe_game::world::HostRequest;
use soe_game::Game;
use soe_godot_host::ui::{apply_host_request, MainMenuState};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn game() -> Game {
    Game::new(LoadOrder::new())
}

fn invoke_global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr31{name}");
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "None");
    let mut operands = vec![s("Game"), s(name), ident("result")];
    for (index, _) in args.iter().enumerate() {
        let arg_name = format!("arg{index}");
        function = function.param(&arg_name, "Var");
        operands.push(ident(&arg_name));
    }
    let bytes = ScriptBuilder::new(&script_name, "Form")
        .function(function.ins(CallStatic, operands).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script_name, "Run", args).unwrap();
    assert!(game.run_until_idle(16));
    game.vm
        .thread_result(thread)
        .expect("completed global call has a result")
}

#[test]
fn scr_31_quit_to_main_menu_queues_request_and_clears_sequence() {
    let mut game = game();
    let mut menu = MainMenuState::default();
    menu.start_sequence("OldIntro".into());

    assert_eq!(invoke_global(&mut game, "QuitToMainMenu", &[]), Value::None);
    let requests = std::mem::take(&mut game.world_mut().host_requests);
    assert_eq!(requests, [HostRequest::QuitToMainMenu]);
    for request in requests {
        apply_host_request(&mut menu, request);
    }
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}

#[test]
fn scr_31_start_title_sequence_forwards_name_to_menu() {
    let mut game = game();
    let mut menu = MainMenuState::default();

    assert_eq!(
        invoke_global(
            &mut game,
            "StartTitleSequence",
            &[Value::string("MQ101Intro")],
        ),
        Value::None
    );
    let requests = std::mem::take(&mut game.world_mut().host_requests);
    assert_eq!(
        requests,
        [HostRequest::StartTitleSequence("MQ101Intro".into())]
    );
    for request in requests {
        apply_host_request(&mut menu, request);
    }
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), Some("MQ101Intro"));
}

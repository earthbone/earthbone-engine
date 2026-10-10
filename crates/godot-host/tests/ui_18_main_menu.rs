// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-18 routes the Game globals through the host's main-menu lifecycle.

use soe_esm::LoadOrder;
use soe_game::world::HostRequest;
use soe_game::Game;
use soe_godot_host::ui::{apply_host_request, MainMenuState};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};
use std::process::Command;

fn game() -> Game {
    Game::new(LoadOrder::new())
}

fn invoke_global(game: &mut Game, name: &str, args: &[Value]) {
    let script_name = format!("Ui18{name}");
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
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn ui_18_quit_to_main_menu_releases_the_game_and_clears_title_sequence() {
    let mut game = game();
    let mut menu = MainMenuState::default();
    menu.start_sequence("Intro".into());

    invoke_global(&mut game, "QuitToMainMenu", &[]);
    let requests = std::mem::take(&mut game.world_mut().host_requests);
    assert_eq!(requests, [HostRequest::QuitToMainMenu]);
    for request in requests {
        apply_host_request(&mut menu, request);
    }

    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}

#[test]
fn ui_18_start_title_sequence_enters_menu_and_preserves_requested_name() {
    let mut game = game();
    let mut menu = MainMenuState::default();
    invoke_global(
        &mut game,
        "StartTitleSequence",
        &[Value::string("MQ101Intro")],
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

#[test]
fn ui_18_title_sequence_animation_plays_on_the_main_menu_in_godot() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .current_dir(&workspace)
        .args([
            "--windowed",
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "ui_18_main_menu_test.gd",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("PASS UI-18 main menu lifecycle"),
        "{stdout}\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

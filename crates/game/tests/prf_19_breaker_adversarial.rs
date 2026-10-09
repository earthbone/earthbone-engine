// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_papyrus::asm::{f, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode;

#[test]
fn prf_19_breaker_timer_modes_replace_only_the_same_registration_and_owner() {
    let mut order = LoadOrder::new();
    order
        .add("Empty.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let mut builder = ScriptBuilder::new("TimerOwner", "Form");
    for function in ["Repeating", "Single"] {
        let native = if function == "Repeating" {
            "RegisterForUpdate"
        } else {
            "RegisterForSingleUpdate"
        };
        builder = builder.function(
            FunctionBuilder::new(function, "None")
                .ins(
                    Opcode::CallMethod,
                    vec![
                        soe_papyrus::asm::ident(native),
                        soe_papyrus::asm::ident("self"),
                        soe_papyrus::asm::ident("::nonevar"),
                        f(3.0),
                    ],
                )
                .ins(Opcode::Return, vec![])
                .build(),
        );
    }
    let script_bytes = builder.bytes();
    game.load_script(&script_bytes).unwrap();
    let first = game.vm.create_object_of("first", "Form");
    let second = game.vm.create_object_of("second", "Form");
    game.vm.attach(first, "TimerOwner").unwrap();
    game.vm.attach(second, "TimerOwner").unwrap();
    game.vm.call(first, "Repeating", &[]).unwrap();
    game.vm.call(first, "Single", &[]).unwrap();
    game.vm.call(second, "Repeating", &[]).unwrap();
    assert!(game.run_until_idle(1));

    let health = game.papyrus_health();
    assert_eq!(health.total.updates.repeating, 2);
    assert_eq!(health.total.updates.single, 1);
    assert_eq!(health.scripts["timerowner"].updates.repeating, 2);
    assert_eq!(health.scripts["timerowner"].updates.single, 1);
    assert!(!health.scripts["timerowner"]
        .events
        .contains_key("onitemadded"));

    let saved = game.save_state().unwrap();
    let mut restored_order = LoadOrder::new();
    restored_order
        .add("Empty.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut restored = Game::new(restored_order);
    restored.load_script(&script_bytes).unwrap();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.papyrus_health().total.updates.repeating, 2);
    assert_eq!(restored.papyrus_health().total.updates.single, 1);
}

#[test]
fn prf_19_breaker_godot_overlay_shows_health_in_both_script_and_plugin_views() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        eprintln!("SKIP PRF-19 overlay: SKYRIM_DATA or SOE_GODOT_BIN is unset");
        return;
    };
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let user = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(format!(".prf-19-user-{}", std::process::id()));
    let mut command = std::process::Command::new(godot);
    command
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script", "prf_19_health_test.gd"])
        .env("XDG_DATA_HOME", &user)
        .env("SKYRIM_DATA", data);
    let output = command.output().unwrap();
    std::fs::remove_dir_all(user).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("prf_19_host_health_and_plugin_overlay: PASS"),
        "{stdout}\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

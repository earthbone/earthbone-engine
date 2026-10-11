// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallStatic, Value};

fn request_screenshot(game: &mut Game, script: &str, path: &str) {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("path", "String")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TakeScreenshot"),
                ident("::nonevar"),
                ident("path"),
            ],
        )
        .build();
    game.load_script(&ScriptBuilder::new(script, "None").function(run).bytes())
        .unwrap();
    let thread = game
        .vm
        .call_global(script, "Run", &[Value::string(path)])
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn hst_19_screenshot_paths_survive_unicode_spaces_duplicates_and_queue_drains() {
    let mut game = Game::new(LoadOrder::new());
    let script = "Hst19PathBreaker";
    let paths = [
        "captures/My Screenshot.png",
        "captures/雪.png",
        "captures/My Screenshot.png",
    ];

    for path in paths {
        request_screenshot(&mut game, script, path);
    }
    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        paths.map(str::to_string)
    );
    assert!(game.world_mut().take_screenshot_requests().is_empty());

    request_screenshot(&mut game, script, "captures/after-drain.png");
    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        vec!["captures/after-drain.png"]
    );
    assert!(game.world_mut().take_screenshot_requests().is_empty());
}

#[test]
fn hst_19_pending_screenshot_requests_are_transient_across_world_save_load() {
    let mut game = Game::new(LoadOrder::new());
    request_screenshot(&mut game, "Hst19SaveBreaker", "captures/pending.png");

    let saved = game.world().save_state();
    let mut restored = Game::new(LoadOrder::new());
    restored.world_mut().load_state(&saved).unwrap();

    assert!(restored.world_mut().take_screenshot_requests().is_empty());
    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        vec!["captures/pending.png"]
    );
}

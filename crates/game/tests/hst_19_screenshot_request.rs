// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-19 verifies that `Debug.TakeScreenshot` forwards the caller path to
//! the host request queue. The CK `Debug_Script` page identifies `Debug` as
//! the global debug-function class.

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn screenshot_script() -> Vec<u8> {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("path", "String")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TakeScreenshot"),
                s("::nonevar"),
                ident("path"),
            ],
        )
        .build();
    ScriptBuilder::new("Hst19ScreenshotProbe", "None")
        .function(run)
        .bytes()
}

#[test]
fn hst_19_debug_take_screenshot_queues_the_requested_path_once() {
    let mut game = Game::new(LoadOrder::new());
    game.start();
    game.load_script(&screenshot_script()).unwrap();
    game.vm
        .call_global(
            "Hst19ScreenshotProbe",
            "Run",
            &[soe_papyrus::Value::string("captures/current.png")],
        )
        .unwrap();
    game.run_until_idle(64);

    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        vec!["captures/current.png".to_string()]
    );
    assert!(game.world_mut().take_screenshot_requests().is_empty());
}

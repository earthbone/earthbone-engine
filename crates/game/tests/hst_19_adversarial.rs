// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn request(game: &mut Game, path: &str) {
    let function = FunctionBuilder::new("Run", "None")
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
    game.load_script(
        &ScriptBuilder::new("Hst19Adversarial", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm
        .call_global(
            "Hst19Adversarial",
            "Run",
            &[soe_papyrus::Value::string(path)],
        )
        .unwrap();
    assert!(game.run_until_idle(64));
}

#[test]
fn hst_19_screenshot_queue_preserves_empty_path_and_repeated_request_order() {
    let mut game = Game::new(LoadOrder::new());
    request(&mut game, "");
    request(&mut game, "first.png");
    request(&mut game, "second.png");
    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        vec!["", "first.png", "second.png"]
    );
    assert!(game.world_mut().take_screenshot_requests().is_empty());
}

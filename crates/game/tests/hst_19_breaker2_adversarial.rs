// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;

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
        &ScriptBuilder::new("Hst19Breaker2", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm
        .call_global("Hst19Breaker2", "Run", &[soe_papyrus::Value::string(path)])
        .unwrap();
    assert!(game.run_until_idle(64));
}

#[test]
fn hst_19_duplicate_requests_remain_distinct_and_ordered() {
    let mut game = Game::new(LoadOrder::new());
    request(&mut game, "same.png");
    request(&mut game, "same.png");
    request(&mut game, "after.png");
    assert_eq!(
        game.world_mut().take_screenshot_requests(),
        vec!["same.png", "same.png", "after.png"]
    );
    assert!(game.world_mut().take_screenshot_requests().is_empty());
}

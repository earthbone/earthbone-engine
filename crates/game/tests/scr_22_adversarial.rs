// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_game::{Game, World};
use soe_esm::LoadOrder;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

#[test]
fn scr_22_take_screenshot_preserves_path_and_consumes_each_request_once() {
    let mut game = Game::new(LoadOrder::new());
    let script = "Scr22ScreenshotProbe";
    let function = FunctionBuilder::new("Run", "None").global().param("path", "String")
        .ins(CallStatic, vec![s("Debug"), s("TakeScreenshot"), ident("::nonevar"), ident("path")])
        .ins(Return, vec![soe_papyrus::asm::none()]).build();
    game.load_script(&ScriptBuilder::new(script, "Form").function(function).bytes()).unwrap();
    for path in ["first.png", "second-shot.png"] {
        game.vm.call_global(script, "Run", &[Value::string(path)]).unwrap();
        game.run_until_idle(64);
    }
    assert_eq!(game.world_mut().take_screenshot_requests(), vec!["first.png", "second-shot.png"]);
    assert!(game.world_mut().take_screenshot_requests().is_empty());
    let _ = World::new(LoadOrder::new());
}

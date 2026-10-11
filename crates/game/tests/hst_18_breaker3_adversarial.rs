// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn read_ref(game: &mut Game, name: &str) -> Value {
    let function = FunctionBuilder::new("Read", "Form")
        .global()
        .local("result", "Form")
        .ins(CallStatic, vec![s("Game"), s(name), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Hst18Breaker3", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("Hst18Breaker3", "Read", &[]).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn hst_18_getters_return_none_for_missing_and_out_of_load_order_references() {
    let mut game = Game::new(LoadOrder::new());
    let invalid = GlobalFormId {
        file: u16::MAX,
        object: u32::MAX,
    };
    game.world_mut().set_crosshair_reference(Some(invalid));
    game.world_mut().set_grabbed_reference(Some(invalid));

    for _ in 0..3 {
        assert_eq!(read_ref(&mut game, "GetCurrentCrosshairRef"), Value::None);
        assert_eq!(read_ref(&mut game, "GetPlayerGrabbedRef"), Value::None);
    }

    game.world_mut().set_crosshair_reference(None);
    game.world_mut().set_grabbed_reference(None);
    assert_eq!(read_ref(&mut game, "GetCurrentCrosshairRef"), Value::None);
    assert_eq!(read_ref(&mut game, "GetPlayerGrabbedRef"), Value::None);
}

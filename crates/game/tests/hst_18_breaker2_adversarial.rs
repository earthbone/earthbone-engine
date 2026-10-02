// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn read_ref(game: &mut Game, name: &str) -> Value {
    let function = FunctionBuilder::new("Read", "Form")
        .global()
        .local("result", "Form")
        .ins(CallStatic, vec![s("Game"), s(name), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Hst18Breaker2", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("Hst18Breaker2", "Read", &[]).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn hst_18_host_references_do_not_survive_engine_save_load() {
    let mut game = Game::new(LoadOrder::new());
    let reference = game.world_mut().player();
    game.world_mut().set_crosshair_reference(Some(reference));
    game.world_mut().set_grabbed_reference(Some(reference));
    let object = game.object_of_form(reference);
    assert_eq!(
        read_ref(&mut game, "GetCurrentCrosshairRef"),
        Value::Object(object)
    );
    assert_eq!(
        read_ref(&mut game, "GetPlayerGrabbedRef"),
        Value::Object(object)
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(LoadOrder::new());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(read_ref(&mut loaded, "GetCurrentCrosshairRef"), Value::None);
    assert_eq!(read_ref(&mut loaded, "GetPlayerGrabbedRef"), Value::None);
}

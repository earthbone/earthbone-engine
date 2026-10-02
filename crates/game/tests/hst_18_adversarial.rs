// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
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
        &ScriptBuilder::new("Hst18Adversarial", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Hst18Adversarial", "Read", &[])
        .unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn hst_18_reference_natives_reflect_repeated_host_updates_and_clear() {
    let mut game = Game::new(LoadOrder::new());
    let reference = game.world_mut().player();
    let object = game.object_of_form(reference);
    assert_eq!(read_ref(&mut game, "GetCurrentCrosshairRef"), Value::None);
    assert_eq!(read_ref(&mut game, "GetPlayerGrabbedRef"), Value::None);

    game.world_mut().set_crosshair_reference(Some(reference));
    game.world_mut().set_grabbed_reference(Some(reference));
    assert_eq!(
        read_ref(&mut game, "GetCurrentCrosshairRef"),
        Value::Object(object)
    );
    assert_eq!(
        read_ref(&mut game, "GetPlayerGrabbedRef"),
        Value::Object(object)
    );

    game.world_mut().set_crosshair_reference(None);
    game.world_mut().set_grabbed_reference(None);
    assert_eq!(read_ref(&mut game, "GetCurrentCrosshairRef"), Value::None);
    assert_eq!(read_ref(&mut game, "GetPlayerGrabbedRef"), Value::None);
}

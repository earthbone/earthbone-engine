// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::HostRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn invoke(game: &mut Game, args: &[Value]) {
    let script = format!("Scr28Breaker2{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None").global();
    let mut operands = vec![s("Game"), s("FadeOutGame"), ident("unused")];
    for (index, _) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, "Var");
        operands.push(ident(&parameter));
    }
    game.load_script(
        &ScriptBuilder::new(&script, "")
            .function(function.ins(CallStatic, operands).build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_28_fade_rejects_missing_and_wrong_typed_signature_arguments() {
    let mut game = Game::new(LoadOrder::new());
    invoke(&mut game, &[]);
    invoke(
        &mut game,
        &[
            Value::string("out"),
            Value::Bool(false),
            Value::Float(0.0),
            Value::Float(0.5),
        ],
    );

    assert!(game.world().host_requests.is_empty());
}

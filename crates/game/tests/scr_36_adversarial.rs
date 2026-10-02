// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    Opcode::{CallMethod, CallStatic, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

const MESSAGE: u32 = 10;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin
        .push_top(Record::new(Tag::new(b"MESG"), MESSAGE).zsub(Tag::new(b"DESC"), "Use the door"));
    let mut order = LoadOrder::new();
    order.add("Scr36.esm", plugin).unwrap();
    Game::new(order)
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn run(game: &mut Game, target: Option<u32>, name: &str, args: &[&str]) -> Value {
    let script = format!("Scr36Probe{}", PROBE_ID.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = if target.is_some() {
        function = function.param("target", "Message");
        vec![s(name), ident("target"), ident("result")]
    } else {
        vec![s("Message"), s(name), ident("result")]
    };
    for (index, _) in args.iter().enumerate() {
        let argument = format!("arg{index}");
        function = function.param(&argument, "String");
        operands.push(ident(&argument));
    }
    let opcode = if target.is_some() {
        CallMethod
    } else {
        CallStatic
    };
    let function = function
        .ins(opcode, operands)
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let mut values = target
        .map(|object| vec![Value::Object(game.object_of_form(id(object)))])
        .unwrap_or_default();
    values.extend(args.iter().map(|arg| Value::string(arg)));
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_36_method_and_global_reset_dispatch_reenable_help_notifications() {
    let mut game = game();
    assert_eq!(
        run(&mut game, Some(MESSAGE), "ShowAsHelpMessage", &["Activate"]),
        Value::None
    );
    assert_eq!(
        run(&mut game, None, "ResetHelpMessage", &["Activate"]),
        Value::None
    );
    assert_eq!(
        run(&mut game, Some(MESSAGE), "ShowAsHelpMessage", &["Activate"]),
        Value::None
    );
    assert_eq!(game.world().notifications, ["Use the door", "Use the door"]);

    run(&mut game, Some(MESSAGE), "ShowAsHelpMessage", &["Jump"]);
    assert_eq!(
        run(&mut game, Some(MESSAGE), "ResetHelpMessage", &["Jump"]),
        Value::None
    );
    run(&mut game, Some(MESSAGE), "ShowAsHelpMessage", &["Jump"]);
    assert_eq!(
        game.world().notifications,
        [
            "Use the door",
            "Use the door",
            "Use the door",
            "Use the door"
        ]
    );
    assert!(game.vm.log.messages().is_empty());
}

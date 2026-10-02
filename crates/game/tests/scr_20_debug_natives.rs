// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-20 contracts for global Debug user-log, quit and cell-center natives.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::DebugHostRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::{ThreadState, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x800).zsub(T(b"EDID"), "TargetCell"));
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.start();
    game
}

fn call(
    game: &mut Game,
    name: &str,
    args: &[Value],
    returns: &str,
) -> (ThreadState, Option<Value>) {
    let class = format!("Scr20Probe{name}{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", returns).global();
    let mut operands = vec![s("Debug"), s(name), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        let ty = match arg {
            Value::Int(_) => "Int",
            Value::Bool(_) => "Bool",
            _ => "String",
        };
        function = function.param(&parameter, ty);
        operands.push(ident(&parameter));
    }
    function = function.local("result", "Var").ins(CallStatic, operands);
    if returns != "None" {
        function = function.ins(Return, vec![ident("result")]);
    }
    game.load_script(
        &ScriptBuilder::new(&class, "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(&class, "Run", args).unwrap();
    game.run_until_idle(64);
    (
        game.vm.thread_state(thread).unwrap(),
        game.vm.thread_result(thread),
    )
}

#[test]
fn scr_20_open_user_log_checks_name_and_duplicate_state() {
    let mut game = game();
    assert_eq!(
        call(&mut game, "OpenUserLog", &[Value::string("quest")], "Bool").1,
        Some(Value::Bool(true))
    );
    assert_eq!(
        call(&mut game, "OpenUserLog", &[Value::string("quest")], "Bool").1,
        Some(Value::Bool(false))
    );
    assert_eq!(
        call(&mut game, "OpenUserLog", &[Value::string("")], "Bool").1,
        Some(Value::Bool(false))
    );
    assert!(game.world().user_logs.contains_key("quest"));
}

#[test]
fn scr_20_trace_user_writes_only_to_an_open_named_log() {
    let mut game = game();
    call(&mut game, "OpenUserLog", &[Value::string("quest")], "Bool");
    assert_eq!(
        call(
            &mut game,
            "TraceUser",
            &[
                Value::string("quest"),
                Value::string("message"),
                Value::Int(0)
            ],
            "Bool"
        )
        .1,
        Some(Value::Bool(true))
    );
    assert_eq!(game.world().user_logs["quest"], vec!["message"]);
    assert_eq!(
        call(&mut game, "CloseUserLog", &[Value::string("quest")], "None").1,
        Some(Value::None)
    );
    assert_eq!(
        call(
            &mut game,
            "TraceUser",
            &[
                Value::string("quest"),
                Value::string("ignored"),
                Value::Int(2)
            ],
            "Bool"
        )
        .1,
        Some(Value::Bool(false))
    );
    assert!(game
        .vm
        .log
        .contains("Debug.TraceUser log is not open: quest"));
}

#[test]
fn scr_20_close_user_log_is_idempotent() {
    let mut game = game();
    call(&mut game, "OpenUserLog", &[Value::string("quest")], "Bool");
    call(&mut game, "CloseUserLog", &[Value::string("quest")], "None");
    call(&mut game, "CloseUserLog", &[Value::string("quest")], "None");
    assert!(!game.world().user_logs.contains_key("quest"));
}

#[test]
fn scr_20_center_on_cell_queues_resolved_destination_and_rejects_unknown_name() {
    let mut game = game();
    let (state, result) = call(
        &mut game,
        "CenterOnCell",
        &[Value::string("TargetCell")],
        "None",
    );
    assert_eq!(state, ThreadState::Done);
    assert_eq!(result, Some(Value::None));
    assert!(matches!(
        game.world().debug_host_requests.as_slice(),
        [DebugHostRequest::CenterOnCell { cell, latent: None }] if *cell == game.form("TargetCell").unwrap()
    ));
    game.world_mut().debug_host_requests.clear();
    call(
        &mut game,
        "CenterOnCell",
        &[Value::string("MissingCell")],
        "None",
    );
    assert!(game.world().debug_host_requests.is_empty());
    assert!(game
        .vm
        .log
        .contains("Debug.CenterOnCell cell not found: MissingCell"));
}

#[test]
fn scr_20_center_on_cell_and_wait_suspends_until_host_completion() {
    let mut game = game();
    let (state, result) = call(
        &mut game,
        "CenterOnCellAndWait",
        &[Value::string("TargetCell")],
        "Float",
    );
    assert!(matches!(state, ThreadState::Latent(_)));
    assert_eq!(result, None);
    let latent = match game.world().debug_host_requests.as_slice() {
        [DebugHostRequest::CenterOnCell {
            latent: Some(latent),
            ..
        }] => *latent,
        _ => panic!("expected the host request to carry the latent waiter"),
    };
    game.vm.resolve_latent(latent, Value::Float(0.0));
    game.run_until_idle(64);
    game.world_mut().debug_host_requests.clear();
    let (state, result) = call(
        &mut game,
        "CenterOnCellAndWait",
        &[Value::string("MissingCell")],
        "Float",
    );
    assert_eq!(state, ThreadState::Done);
    assert_eq!(result, Some(Value::Float(-1.0)));
    assert!(game.world().debug_host_requests.is_empty());
    assert!(game
        .vm
        .log
        .contains("Debug.CenterOnCellAndWait cell not found: MissingCell"));
}

#[test]
fn scr_20_quit_game_queues_host_exit_request() {
    let mut game = game();
    let (state, result) = call(&mut game, "QuitGame", &[], "None");
    assert_eq!(state, ThreadState::Done);
    assert_eq!(result, Some(Value::None));
    assert_eq!(
        game.world().debug_host_requests,
        vec![DebugHostRequest::Quit]
    );
}

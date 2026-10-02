// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, world::DebugHostRequest};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use soe_papyrus::{ThreadState, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x800).zsub(T(b"EDID"), "TargetCell"));
    let mut order = LoadOrder::new();
    order.add("Scr20.esm", plugin).unwrap();
    Game::new(order)
}

fn call(
    game: &mut Game,
    method: &str,
    arg: &str,
    result_type: &str,
) -> (ThreadState, Option<Value>) {
    let script = format!("Scr20Breaker{method}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            FunctionBuilder::new("Run", result_type)
                .global()
                .param("arg", "String")
                .local("result", "Var")
                .ins(
                    CallStatic,
                    vec![s("Debug"), s(method), ident("result"), ident("arg")],
                )
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::string(arg)])
        .unwrap();
    game.run_until_idle(64);
    (
        game.vm.thread_state(thread).unwrap(),
        game.vm.thread_result(thread),
    )
}

#[test]
fn scr_20_user_log_can_be_closed_and_reopened_without_stale_lines() {
    let mut game = game();
    assert_eq!(
        call(&mut game, "OpenUserLog", "case", "Bool").1,
        Some(Value::Bool(true))
    );
    game.world_mut()
        .user_logs
        .get_mut("case")
        .unwrap()
        .push("transient".into());
    assert_eq!(
        call(&mut game, "CloseUserLog", "case", "None").1,
        Some(Value::None)
    );
    assert_eq!(
        call(&mut game, "OpenUserLog", "case", "Bool").1,
        Some(Value::Bool(true))
    );
    assert!(game.world().user_logs["case"].is_empty());
}

#[test]
fn scr_20_center_on_cell_lookup_is_case_insensitive_and_nonlatent() {
    let mut game = game();
    let (state, result) = call(&mut game, "CenterOnCell", "TargetCell", "None");
    assert_eq!(state, ThreadState::Done);
    assert_eq!(result, Some(Value::None));
    assert!(matches!(
        game.world().debug_host_requests.as_slice(),
        [DebugHostRequest::CenterOnCell { latent: None, .. }]
    ));
}

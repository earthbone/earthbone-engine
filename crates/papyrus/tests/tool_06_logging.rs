// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! VM diagnostic routing preserves the existing script log contract.

use std::sync::{Arc, Mutex};

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode;
use soe_papyrus::{Log, Vm};

#[test]
fn tool_06_papyrus_log_preserves_entries_and_routes_severity() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let output = captured.clone();
    let (subscriber, control) = soe_trace::logging::subscriber("trace", None, move |line| {
        output.lock().unwrap().push(line.clone());
    })
    .unwrap();
    let mut log = Log::default();
    soe_trace::tracing::subscriber::with_default(subscriber, || {
        log.push(1.0, "plain text");
        log.push(2.0, "warning: a warning");
        log.push(3.0, "error: an error");
    });
    control.flush_all().unwrap();
    assert_eq!(
        log.messages(),
        ["plain text", "warning: a warning", "error: an error"]
    );
    let output = captured.lock().unwrap();
    for (message, level) in [
        ("plain text", "INFO"),
        ("warning: a warning", "WARN"),
        ("error: an error", "ERROR"),
    ] {
        assert!(output
            .iter()
            .any(|line| line.level.as_str() == level && line.fields["message"] == message));
    }
}

#[test]
fn tool_06_vm_error_carries_script_and_thread() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let output = captured.clone();
    let (subscriber, control) = soe_trace::logging::subscriber("error", None, move |line| {
        output.lock().unwrap().push(line.clone());
    })
    .unwrap();
    soe_trace::tracing::subscriber::with_default(subscriber, || {
        let bytes = ScriptBuilder::new("Tool06", "")
            .function(
                FunctionBuilder::new("Fail", "Int")
                    .global()
                    .local("result", "Int")
                    .ins(Opcode::IDiv, vec![ident("result"), i(1), i(0)])
                    .ins(Opcode::Return, vec![ident("result")])
                    .build(),
            )
            .bytes();
        let mut vm = Vm::new();
        vm.load_bytes(&bytes).unwrap();
        vm.call_global("Tool06", "Fail", &[]).unwrap();
        vm.run_until_idle(100);
        assert!(vm.log.contains("error:"));
    });
    control.flush_all().unwrap();
    let output = captured.lock().unwrap();
    assert!(output.iter().any(|line| line
        .fields
        .get("script")
        .is_some_and(|script| script.starts_with("Tool06.Fail"))
        && line.fields.contains_key("thread")));
}

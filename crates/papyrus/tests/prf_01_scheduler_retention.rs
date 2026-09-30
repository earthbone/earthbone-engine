// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::Return;
use soe_papyrus::{ThreadId, ThreadState, Value, Vm, MAX_COMPLETED_THREAD_HISTORY};

fn current_rss_bytes() -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("read process status");
    let resident_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|value| value.parse::<usize>().ok())
        .expect("find resident memory");
    resident_kib * 1024
}

#[test]
fn prf_01_script_soak_stays_within_memory_budget_and_retention_policy() {
    let bytes = ScriptBuilder::new("Retention", "")
        .function(
            FunctionBuilder::new("Tick", "Int")
                .ins(Return, vec![i(7)])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).expect("script loads");
    let baseline = current_rss_bytes();
    let mut oldest = None;
    let mut newest = None;

    for _ in 0..1600 {
        for _ in 0..256 {
            let id = vm
                .call_global("Retention", "Tick", &[])
                .expect("call starts");
            oldest.get_or_insert(id);
            newest = Some(id);
        }
        vm.run(256);
        assert_eq!(vm.live_threads(), 0, "each soak batch completes");
    }

    let oldest = oldest.expect("soak starts threads");
    let newest = newest.expect("soak starts threads");
    assert_eq!(
        vm.thread_state(oldest),
        None,
        "results older than the retention window are released"
    );
    assert_eq!(
        vm.thread_state(ThreadId(newest.0 - MAX_COMPLETED_THREAD_HISTORY as u64)),
        None,
        "the history cap releases the entry immediately before its window"
    );
    assert_eq!(
        vm.thread_state(ThreadId(
            newest.0 - (MAX_COMPLETED_THREAD_HISTORY as u64 - 1)
        )),
        Some(ThreadState::Done),
        "the history cap keeps its oldest retained entry"
    );
    assert_eq!(vm.thread_state(newest), Some(ThreadState::Done));
    assert_eq!(vm.thread_result(newest), Some(Value::Int(7)));
    let growth = current_rss_bytes().saturating_sub(baseline);
    assert!(growth <= 64 * 1024 * 1024, "RSS growth was {growth} bytes");
}

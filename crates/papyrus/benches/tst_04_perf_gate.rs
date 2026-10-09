#![allow(clippy::expect_used, clippy::panic)]

use iai_callgrind::{
    library_benchmark, library_benchmark_group, main, Callgrind, EventKind, LibraryBenchmarkConfig,
};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};
use soe_papyrus::{ThreadId, Value, Vm};
use std::hint::black_box;

const LOOP_COUNT: i32 = 100;
const MAX_PARSE_INSTRUCTIONS: u64 = 500_000;
const MAX_VM_INSTRUCTIONS: u64 = 2_000_000;

fn script_bytes() -> Vec<u8> {
    let mut function = FunctionBuilder::new("Count", "Int").local("index", "Int");
    for _ in 0..256 {
        function = function.ins(Opcode::Nop, Vec::new());
    }
    let parse_script = ScriptBuilder::new("Tst04Parser", "")
        .function(function.ins(Opcode::Return, vec![i(0)]).build())
        .build();
    soe_papyrus::asm::write(&parse_script)
}

fn vm_script_bytes() -> Vec<u8> {
    let count = FunctionBuilder::new("Count", "Int")
        .param("limit", "Int")
        .local("index", "Int")
        .local("total", "Int")
        .local("continue", "Bool")
        .ins(Opcode::Assign, vec![ident("index"), i(0)])
        .ins(Opcode::Assign, vec![ident("total"), i(0)])
        .ins(
            Opcode::CmpLt,
            vec![ident("continue"), ident("index"), ident("limit")],
        )
        .ins(Opcode::JmpF, vec![ident("continue"), i(4)])
        .ins(
            Opcode::IAdd,
            vec![ident("total"), ident("total"), ident("index")],
        )
        .ins(Opcode::IAdd, vec![ident("index"), ident("index"), i(1)])
        .ins(Opcode::Jmp, vec![i(-4)])
        .ins(Opcode::Return, vec![ident("total")])
        .build();
    let script = ScriptBuilder::new("Tst04Vm", "").function(count).build();
    soe_papyrus::asm::write(&script)
}

fn setup_vm(bytes: Vec<u8>) -> (Vm, ThreadId) {
    let mut vm = Vm::new();
    let bytes = bytes.into_boxed_slice();
    vm.load_bytes(&bytes).expect("synthetic PEX loads");
    let thread = vm
        .call_global("Tst04Vm", "Count", &[Value::Int(LOOP_COUNT)])
        .expect("hot loop starts");
    (vm, thread)
}

#[library_benchmark(
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::default().hard_limits([(EventKind::Ir, MAX_PARSE_INSTRUCTIONS)])
    )
)]
#[bench::parse_256_nops(args = (), setup = script_bytes)]
fn bench_pex_parse(bytes: Vec<u8>) -> usize {
    black_box(
        pex::parse(&bytes)
            .expect("synthetic PEX parses")
            .object()
            .states
            .len(),
    )
}

#[library_benchmark(
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::default().hard_limits([(EventKind::Ir, MAX_VM_INSTRUCTIONS)])
    )
)]
#[bench::execute_100_loop_iterations(args = (vm_script_bytes()), setup = setup_vm)]
fn bench_vm_run((mut vm, thread): (Vm, ThreadId)) -> Value {
    assert!(vm.run_until_idle(100_000), "VM hot loop reaches idle");
    black_box(vm.thread_result(thread).expect("hot loop returns a result"))
}

library_benchmark_group!(
    name = tst_04_papyrus_parser_and_vm;
    benchmarks = bench_pex_parse, bench_vm_run
);

main!(library_benchmark_groups = tst_04_papyrus_parser_and_vm);

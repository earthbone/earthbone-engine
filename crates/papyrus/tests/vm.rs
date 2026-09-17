// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every test builds a script with the assembler, serializes it to PEX bytes,
//! and loads those bytes, so the reader and the VM are both on the path.

use std::cell::RefCell;
use std::rc::Rc;

use soe_papyrus::asm::{b, f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, ByteOrder, Opcode::*, PexError};
use soe_papyrus::{NativeReturn, ThreadState, Value, Vm, VmError};

fn vm_with(scripts: Vec<Vec<u8>>) -> Vm {
    let mut vm = Vm::new();
    for bytes in scripts {
        vm.load_bytes(&bytes).expect("script loads");
    }
    vm
}

/// A script that computes `1 + 2 + ... + n` with a loop, exercising the
/// arithmetic, comparison and jump opcodes.
fn sum_script() -> Vec<u8> {
    let sum = FunctionBuilder::new("Sum", "Int")
        .param("n", "Int")
        .local("i", "Int")
        .local("acc", "Int")
        .local("::temp0", "Bool")
        .ins(Assign, vec![ident("acc"), i(0)])
        .ins(Assign, vec![ident("i"), i(1)])
        .ins(CmpLte, vec![ident("::temp0"), ident("i"), ident("n")])
        .ins(JmpF, vec![ident("::temp0"), i(4)])
        .ins(IAdd, vec![ident("acc"), ident("acc"), ident("i")])
        .ins(IAdd, vec![ident("i"), ident("i"), i(1)])
        .ins(Jmp, vec![i(-4)])
        .ins(Return, vec![ident("acc")])
        .lines(&[10, 11, 12, 12, 13, 14, 12, 16])
        .build();
    ScriptBuilder::new("Summer", "").function(sum).bytes()
}

fn run_to_result(vm: &mut Vm, tid: soe_papyrus::ThreadId) -> Value {
    assert!(vm.run_until_idle(100_000), "the VM should go idle");
    vm.thread_result(tid).expect("thread finished")
}

#[test]
fn pex_round_trips_in_both_byte_orders() {
    for order in [ByteOrder::Big, ByteOrder::Little] {
        let script = ScriptBuilder::new("Summer", "")
            .byte_order(order)
            .auto_property("Speed", "Float", f(1.5))
            .variable("::count", "Int", i(3))
            .function(
                FunctionBuilder::new("Sum", "Int")
                    .param("n", "Int")
                    .local("acc", "Int")
                    .ins(IAdd, vec![ident("acc"), ident("n"), i(1)])
                    .ins(
                        CallStatic,
                        vec![
                            s("Debug"),
                            s("Notification"),
                            ident("::nonevar"),
                            s("hi"),
                            ident("acc"),
                        ],
                    )
                    .ins(Return, vec![ident("acc")])
                    .lines(&[3, 4, 5])
                    .build(),
            )
            .function_in("Busy", FunctionBuilder::new("OnHit", "None").build())
            .build();
        let bytes = soe_papyrus::asm::write(&script);
        let back = pex::parse(&bytes).expect("parses");
        assert_eq!(back.header.byte_order, order);
        assert_eq!(back, script);
        let obj = back.object();
        assert_eq!(
            obj.property("speed").unwrap().auto_var.as_deref(),
            Some("::Speed_var")
        );
        let sum = obj.function_in("", "sum").unwrap();
        assert_eq!(
            sum.code[1].args.len(),
            5,
            "varargs come back after the fixed operands"
        );
        assert_eq!(sum.line_of(1), Some(4));
    }
}

#[test]
fn magic_in_the_wrong_order_is_the_other_endianness() {
    let big = pex::parse(&sum_script()).unwrap();
    assert_eq!(big.header.byte_order, ByteOrder::Big);
    assert_eq!(&big.header.source, "Summer.psc");
}

#[test]
fn malformed_files_fail_without_panicking() {
    let bytes = sum_script();
    for len in 0..bytes.len() {
        let err = pex::parse(&bytes[..len]).expect_err("a truncated file is an error");
        assert!(matches!(err, PexError::Truncated { .. }), "at {len}: {err}");
    }
    let mut bad = bytes.clone();
    bad[0] = 0;
    assert!(matches!(pex::parse(&bad), Err(PexError::BadMagic(_))));
    let mut fo4 = bytes.clone();
    fo4[5] = 9;
    assert!(matches!(
        pex::parse(&fo4),
        Err(PexError::UnsupportedVersion { major: 3, minor: 9 })
    ));
    // Every single-byte mutation either parses or errors, never panics.
    for pos in 0..bytes.len() {
        let mut m = bytes.clone();
        m[pos] ^= 0xFF;
        let _ = pex::parse(&m);
    }
}

#[test]
fn loops_and_integer_arithmetic() {
    let mut vm = vm_with(vec![sum_script()]);
    let tid = vm.call_global("Summer", "Sum", &[Value::Int(10)]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(55));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn float_arithmetic_negation_not_and_division_by_zero() {
    let calc = FunctionBuilder::new("Calc", "Float")
        .local("a", "Float")
        .local("b", "Int")
        .local("c", "Bool")
        .ins(FAdd, vec![ident("a"), f(1.5), f(2.0)])
        .ins(FMul, vec![ident("a"), ident("a"), f(2.0)])
        .ins(FSub, vec![ident("a"), ident("a"), f(1.0)])
        .ins(FDiv, vec![ident("a"), ident("a"), f(2.0)])
        .ins(FNeg, vec![ident("a"), ident("a")])
        .ins(IMod, vec![ident("b"), i(7), i(3)])
        .ins(INeg, vec![ident("b"), ident("b")])
        .ins(Not, vec![ident("c"), ident("b")])
        .ins(IDiv, vec![ident("b"), i(1), i(0)])
        .ins(Return, vec![ident("a")])
        .build();
    let script = ScriptBuilder::new("Calc", "").function(calc).bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Calc", "Calc", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Float(-3.0));
    assert!(vm.log.contains("integer division by zero"));
}

#[test]
fn casts_follow_the_declared_type_of_the_destination() {
    let cast = FunctionBuilder::new("Casts", "String")
        .local("fl", "Float")
        .local("n", "Int")
        .local("flag", "Bool")
        .local("text", "String")
        .local("obj", "Actor")
        .ins(Cast, vec![ident("fl"), i(3)])
        .ins(Cast, vec![ident("n"), f(2.9)])
        .ins(Cast, vec![ident("flag"), s("yes")])
        .ins(Cast, vec![ident("obj"), i(5)])
        .ins(Cast, vec![ident("text"), ident("fl")])
        .ins(StrCat, vec![ident("text"), ident("text"), s("|")])
        .ins(StrCat, vec![ident("text"), ident("text"), ident("n")])
        .ins(StrCat, vec![ident("text"), ident("text"), ident("flag")])
        .ins(StrCat, vec![ident("text"), ident("text"), ident("obj")])
        .ins(Return, vec![ident("text")])
        .build();
    let script = ScriptBuilder::new("Caster", "").function(cast).bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Caster", "Casts", &[]).unwrap();
    assert_eq!(
        run_to_result(&mut vm, tid),
        Value::string("3.000000|2TrueNone")
    );
}

#[test]
fn comparisons_and_conditional_jumps() {
    let cmp = FunctionBuilder::new("Cmp", "Int")
        .local("r", "Int")
        .local("t", "Bool")
        .ins(CmpEq, vec![ident("t"), s("Abc"), s("aBC")])
        .ins(JmpF, vec![ident("t"), i(2)])
        .ins(IAdd, vec![ident("r"), ident("r"), i(1)])
        .ins(CmpLt, vec![ident("t"), i(1), f(1.5)])
        .ins(JmpT, vec![ident("t"), i(2)])
        .ins(IAdd, vec![ident("r"), ident("r"), i(100)])
        .ins(CmpGte, vec![ident("t"), i(2), i(2)])
        .ins(JmpF, vec![ident("t"), i(2)])
        .ins(IAdd, vec![ident("r"), ident("r"), i(10)])
        .ins(CmpGt, vec![ident("t"), i(2), i(2)])
        .ins(JmpF, vec![ident("t"), i(2)])
        .ins(IAdd, vec![ident("r"), ident("r"), i(1000)])
        .ins(Return, vec![ident("r")])
        .build();
    let script = ScriptBuilder::new("Cmp", "").function(cmp).bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Cmp", "Cmp", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(11));
}

#[test]
fn arrays_are_shared_references_with_bounds_checks() {
    let arr = FunctionBuilder::new("Arr", "Int")
        .local("xs", "Int[]")
        .local("ys", "Int[]")
        .local("n", "Int")
        .local("found", "Int")
        .local("r", "Int")
        .ins(ArrayCreate, vec![ident("xs"), i(3)])
        .ins(ArraySetElement, vec![ident("xs"), i(1), i(7)])
        .ins(Assign, vec![ident("ys"), ident("xs")])
        .ins(ArraySetElement, vec![ident("ys"), i(2), f(9.5)])
        .ins(ArrayLength, vec![ident("n"), ident("xs")])
        .ins(ArrayGetElement, vec![ident("r"), ident("xs"), i(2)])
        .ins(IMul, vec![ident("r"), ident("r"), i(100)])
        .ins(IAdd, vec![ident("r"), ident("r"), ident("n")])
        .ins(
            ArrayFindElement,
            vec![ident("xs"), ident("found"), i(7), i(0)],
        )
        .ins(IMul, vec![ident("found"), ident("found"), i(10)])
        .ins(IAdd, vec![ident("r"), ident("r"), ident("found")])
        .ins(
            ArrayRFindElement,
            vec![ident("xs"), ident("found"), i(0), i(-1)],
        )
        .ins(IMul, vec![ident("found"), ident("found"), i(1000)])
        .ins(IAdd, vec![ident("r"), ident("r"), ident("found")])
        .ins(
            ArrayFindElement,
            vec![ident("xs"), ident("found"), i(42), i(0)],
        )
        .ins(IMul, vec![ident("found"), ident("found"), i(10000)])
        .ins(IAdd, vec![ident("r"), ident("r"), ident("found")])
        .ins(ArrayGetElement, vec![ident("n"), ident("xs"), i(3)])
        .ins(Return, vec![ident("r")])
        .build();
    let script = ScriptBuilder::new("Arr", "").function(arr).bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Arr", "Arr", &[]).unwrap();
    // 9 * 100 + 3 + 1 * 10 + 0 * 1000 + (-1) * 10000
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(903 + 10 - 10000));
    assert!(vm.log.contains("array index 3 out of range, length 3"));
}

fn greeter_scripts() -> Vec<Vec<u8>> {
    let parent = ScriptBuilder::new("Parent", "")
        .variable("::who", "String", s("parent"))
        .function(
            FunctionBuilder::new("Greet", "String")
                .ins(Return, vec![ident("::who")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Twice", "Int")
                .param("x", "Int")
                .local("r", "Int")
                .ins(IMul, vec![ident("r"), ident("x"), i(2)])
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    let child = ScriptBuilder::new("Child", "Parent")
        .function(
            FunctionBuilder::new("Greet", "String")
                .local("base", "String")
                .ins(CallParent, vec![s("Greet"), ident("base")])
                .ins(StrCat, vec![ident("base"), ident("base"), s("+child")])
                .ins(Return, vec![ident("base")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Quad", "Int")
                .param("x", "Int")
                .local("r", "Int")
                .ins(
                    CallMethod,
                    vec![s("Twice"), ident("self"), ident("r"), ident("x")],
                )
                .ins(
                    CallMethod,
                    vec![s("Twice"), ident("self"), ident("r"), ident("r")],
                )
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    vec![parent, child]
}

#[test]
fn method_calls_resolve_through_the_parent_chain() {
    let mut vm = vm_with(greeter_scripts());
    let obj = vm.create_object("Bob");
    vm.attach(obj, "Child").unwrap();
    let tid = vm.call(obj, "Greet", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::string("parent+child"));
    let tid = vm.call(obj, "Quad", &[Value::Int(3)]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(12));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn calling_a_method_on_none_logs_and_continues() {
    let script = ScriptBuilder::new("Caller", "")
        .function(
            FunctionBuilder::new("Go", "Int")
                .local("target", "Parent")
                .local("r", "Int")
                .ins(
                    CallMethod,
                    vec![s("Twice"), ident("target"), ident("r"), i(1)],
                )
                .ins(IAdd, vec![ident("r"), ident("r"), i(5)])
                .ins(Return, vec![ident("r")])
                .lines(&[7, 8, 9])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Caller", "Go", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(5));
    let msg = vm.log.messages().join("\n");
    assert!(
        msg.contains("cannot call Twice() on a None object"),
        "{msg}"
    );
    assert!(msg.contains("Caller.Go line 7"), "{msg}");
}

#[test]
fn auto_properties_and_getter_setter_properties() {
    let getter = FunctionBuilder::new("get", "Int")
        .local("r", "Int")
        .ins(IMul, vec![ident("r"), ident("::val"), i(2)])
        .ins(Return, vec![ident("r")])
        .build();
    let setter = FunctionBuilder::new("set", "None")
        .param("v", "Int")
        .ins(Assign, vec![ident("::val"), ident("v")])
        .build();
    let script = ScriptBuilder::new("Props", "")
        .variable("::val", "Int", i(0))
        .auto_property("Health", "Float", f(50.0))
        .property("Doubled", "Int", Some(getter), Some(setter))
        .function(
            FunctionBuilder::new("Read", "Float")
                .local("h", "Float")
                .local("d", "Int")
                .local("r", "Float")
                .ins(PropSet, vec![s("Doubled"), ident("self"), i(21)])
                .ins(PropGet, vec![s("Doubled"), ident("self"), ident("d")])
                .ins(PropGet, vec![s("Health"), ident("self"), ident("h")])
                .ins(PropSet, vec![s("Health"), ident("self"), f(10.0)])
                .ins(Cast, vec![ident("r"), ident("d")])
                .ins(FAdd, vec![ident("r"), ident("r"), ident("h")])
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let obj = vm.create_object("Thing");
    vm.attach(obj, "Props").unwrap();
    assert_eq!(
        vm.get_property(obj, "Props", "Health"),
        Some(Value::Float(50.0))
    );
    let tid = vm.call(obj, "Read", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Float(92.0));
    assert_eq!(
        vm.get_property(obj, "Props", "Health"),
        Some(Value::Float(10.0))
    );
    assert_eq!(vm.get_var(obj, "Props", "::val"), Some(Value::Int(21)));
    vm.set_property(obj, "Props", "Health", Value::Float(1.0))
        .unwrap();
    assert_eq!(
        vm.get_property(obj, "Props", "Health"),
        Some(Value::Float(1.0))
    );
}

/// `Vm::set_property` and `Vm::get_property` fill and read a full property
/// from outside a running thread, the way the loader fills a form's `VMAD`
/// values, by running the compiled setter or getter to completion. This is
/// the vanilla trap family's shape: `TriggerCount` stores into a variable
/// distinct from the property name and doubles it on the way in, and
/// `TriggerType` is read-only, a getter with no setter.
#[test]
fn set_property_and_get_property_run_the_compiled_setter_and_getter() {
    let setter = FunctionBuilder::new("set", "None")
        .param("v", "Int")
        .ins(IMul, vec![ident("::stored"), ident("v"), i(2)])
        .build();
    let getter = FunctionBuilder::new("get", "Int")
        .ins(Return, vec![ident("::stored")])
        .build();
    let read_only_getter = FunctionBuilder::new("get", "Int")
        .ins(Return, vec![ident("::fixed")])
        .build();
    let script = ScriptBuilder::new("TrapTriggerBase", "")
        .variable("::stored", "Int", i(0))
        .variable("::fixed", "Int", i(7))
        .property("TriggerCount", "Int", Some(getter), Some(setter))
        .property("TriggerType", "Int", Some(read_only_getter), None)
        .function(
            FunctionBuilder::new("ReadStored", "Int")
                .ins(Return, vec![ident("::stored")])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let obj = vm.create_object("Trap01");
    vm.attach(obj, "TrapTriggerBase").unwrap();

    vm.set_property(obj, "TrapTriggerBase", "TriggerCount", Value::Int(21))
        .unwrap();
    let tid = vm.call(obj, "ReadStored", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(42));
    assert_eq!(
        vm.get_property(obj, "TrapTriggerBase", "TriggerCount"),
        Some(Value::Int(42))
    );

    assert_eq!(
        vm.set_property(obj, "TrapTriggerBase", "TriggerType", Value::Int(1)),
        Err(VmError::ReadOnlyProperty {
            class: "TrapTriggerBase".into(),
            name: "TriggerType".into(),
        })
    );
    assert_eq!(
        vm.get_property(obj, "TrapTriggerBase", "TriggerType"),
        Some(Value::Int(7))
    );
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn a_latent_native_suspends_the_thread_two_frames_deep_and_resumes() {
    let script = ScriptBuilder::new("Lat", "")
        .function(
            FunctionBuilder::new("Outer", "Int")
                .local("r", "Int")
                .ins(CallStatic, vec![s("Lat"), s("Inner"), ident("r")])
                .ins(IAdd, vec![ident("r"), ident("r"), i(1)])
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Inner", "Int")
                .local("v", "Int")
                .ins(CallStatic, vec![s("Host"), s("Fetch"), ident("v")])
                .ins(IMul, vec![ident("v"), ident("v"), i(2)])
                .ins(Return, vec![ident("v")])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let pending: Rc<RefCell<Option<soe_papyrus::vm::LatentId>>> = Rc::new(RefCell::new(None));
    let p = pending.clone();
    vm.natives.register_global(
        "Host",
        "Fetch",
        Box::new(move |call| {
            let id = call.defer();
            *p.borrow_mut() = Some(id);
            NativeReturn::Latent(id)
        }),
    );
    let tid = vm.call_global("Lat", "Outer", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    let id = pending.borrow().expect("the native was called");
    assert_eq!(vm.thread_state(tid), Some(ThreadState::Latent(id)));
    assert_eq!(vm.thread_result(tid), None);
    assert!(vm.resolve_latent(id, Value::Int(20)));
    assert!(
        !vm.resolve_latent(id, Value::Int(20)),
        "a latent resolves once"
    );
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(41));
}

#[test]
fn utility_wait_sleeps_until_the_clock_passes() {
    let script = ScriptBuilder::new("Sleepy", "")
        .function(
            FunctionBuilder::new("Nap", "Int")
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("Wait"), ident("::nonevar"), f(1.5)],
                )
                .ins(Return, vec![i(7)])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Sleepy", "Nap", &[]).unwrap();
    vm.run_until_idle(1000);
    assert_eq!(vm.thread_state(tid), Some(ThreadState::Sleeping(1.5)));
    vm.advance(1.0);
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), None);
    vm.advance(0.6);
    assert_eq!(run_to_result(&mut vm, tid), Value::Int(7));
}

#[test]
fn mock_natives_notification_and_a_method_reached_through_declared_parents() {
    let script = ScriptBuilder::new("MyActor", "Actor")
        .auto_property("Greeting", "String", s("hello"))
        .function(
            FunctionBuilder::new("Hail", "String")
                .local("name", "String")
                .local("msg", "String")
                .ins(
                    CallMethod,
                    vec![s("GetDisplayName"), ident("self"), ident("name")],
                )
                .ins(PropGet, vec![s("Greeting"), ident("self"), ident("msg")])
                .ins(StrCat, vec![ident("msg"), ident("msg"), s(" ")])
                .ins(StrCat, vec![ident("msg"), ident("msg"), ident("name")])
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Notification"),
                        ident("::nonevar"),
                        ident("msg"),
                    ],
                )
                .ins(Return, vec![ident("msg")])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let shown: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = shown.clone();
    assert!(vm.natives.register_global(
        "Debug",
        "Notification",
        Box::new(move |call| {
            sink.borrow_mut().push(call.arg(0).to_string());
            NativeReturn::Value(Value::None)
        }),
    ));
    assert!(!vm.natives.register_global(
        "debug",
        "NOTIFICATION",
        Box::new(|_| NativeReturn::Value(Value::None))
    ));
    assert_eq!(vm.natives.duplicates.len(), 1);
    vm.natives.declare_parent("Actor", "ObjectReference");
    vm.natives.register_method(
        "ObjectReference",
        "GetDisplayName",
        Box::new(|call| {
            let who = call.self_object.map(|o| o.0).unwrap_or(0);
            NativeReturn::Value(Value::string(&format!("object {who}")))
        }),
    );
    let obj = vm.create_object("Lydia");
    vm.attach(obj, "MyActor").unwrap();
    let tid = vm.call(obj, "Hail", &[]).unwrap();
    assert_eq!(run_to_result(&mut vm, tid), Value::string("hello object 1"));
    assert_eq!(shown.borrow().as_slice(), ["hello object 1"]);
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn events_reach_every_script_and_states_change_which_function_runs() {
    let script = ScriptBuilder::new("Guard", "")
        .variable("::hits", "Int", i(0))
        .function(
            FunctionBuilder::new("OnHit", "String")
                .ins(IAdd, vec![ident("::hits"), ident("::hits"), i(1)])
                .ins(Return, vec![s("idle")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Alert", "None")
                .ins(
                    CallMethod,
                    vec![s("GotoState"), ident("self"), ident("::nonevar"), s("Busy")],
                )
                .build(),
        )
        .function_in(
            "Busy",
            FunctionBuilder::new("OnHit", "String")
                .ins(IAdd, vec![ident("::hits"), ident("::hits"), i(10)])
                .ins(Return, vec![s("busy")])
                .build(),
        )
        .function_in(
            "Busy",
            FunctionBuilder::new("OnBeginState", "None")
                .ins(IAdd, vec![ident("::hits"), ident("::hits"), i(100)])
                .build(),
        )
        .bytes();
    let other = ScriptBuilder::new("Bystander", "")
        .variable("::seen", "Bool", b(false))
        .function(
            FunctionBuilder::new("OnHit", "None")
                .ins(Assign, vec![ident("::seen"), b(true)])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script, other]);
    let obj = vm.create_object("Whiterun Guard");
    vm.attach(obj, "Guard").unwrap();
    vm.attach(obj, "Bystander").unwrap();
    let threads = vm.send_event(obj, "OnHit", &[]);
    assert_eq!(threads.len(), 2);
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(threads[0]), Some(Value::string("idle")));
    assert_eq!(
        vm.get_var(obj, "Bystander", "::seen"),
        Some(Value::Bool(true))
    );
    let tid = vm.call(obj, "Alert", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::None));
    assert_eq!(vm.state_of(obj, "Guard").as_deref(), Some("Busy"));
    let threads = vm.send_event(obj, "OnHit", &[]);
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(threads[0]), Some(Value::string("busy")));
    assert_eq!(vm.get_var(obj, "Guard", "::hits"), Some(Value::Int(111)));
}

#[test]
fn fallout_4_opcodes_abort_the_thread_with_the_line() {
    let script = ScriptBuilder::new("Fo4", "")
        .function(
            FunctionBuilder::new("Bad", "Int")
                .local("r", "Int")
                .ins(Assign, vec![ident("r"), i(1)])
                .ins(ArrayClear, vec![ident("r")])
                .ins(Return, vec![ident("r")])
                .lines(&[20, 21, 22])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Fo4", "Bad", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_state(tid), Some(ThreadState::Done));
    let msg = vm.log.messages().join("\n");
    assert!(
        msg.contains("ArrayClear") && msg.contains("Fo4.Bad line 21"),
        "{msg}"
    );
}

#[test]
fn a_runaway_recursion_hits_the_depth_limit() {
    let script = ScriptBuilder::new("Rec", "")
        .function(
            FunctionBuilder::new("Down", "Int")
                .local("r", "Int")
                .ins(CallStatic, vec![s("Rec"), s("Down"), ident("r")])
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    let mut vm = vm_with(vec![script]);
    let tid = vm.call_global("Rec", "Down", &[]).unwrap();
    assert!(vm.run_until_idle(100_000));
    assert_eq!(vm.thread_state(tid), Some(ThreadState::Done));
    assert!(vm.log.contains("stack overflow"));
}

#[test]
fn host_api_errors_are_typed() {
    let mut vm = vm_with(vec![sum_script()]);
    assert!(vm.attach(soe_papyrus::ObjectId(99), "Summer").is_err());
    let obj = vm.create_object("x");
    assert!(vm.attach(obj, "Missing").is_err());
    assert!(vm.call_global("Summer", "Nope", &[]).is_err());
    assert!(vm.load_bytes(b"junk").is_err());
    let _ = none();
}

#[test]
fn real_scripts_from_the_game_load_when_present() {
    // The vanilla scripts ship inside `Skyrim - Misc.bsa` under `scripts/`.
    // `SOE_PEX_DIR` points at a loose extraction if one exists (a plain
    // install has none), otherwise the scripts are read straight out of the
    // archive through `soe_bsa`.
    let mut scripts: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(d) = std::env::var_os("SOE_PEX_DIR") {
        let dir = std::path::PathBuf::from(d);
        if !dir.is_dir() {
            eprintln!("no script folder at {}, skipping", dir.display());
            return;
        }
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(&dir, &mut files);
        for p in files {
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pex")) {
                let bytes = std::fs::read(&p).unwrap();
                scripts.push((p.display().to_string(), bytes));
            }
        }
    } else if let Some(d) = std::env::var_os("SKYRIM_DATA") {
        let data = std::path::PathBuf::from(d);
        let archive =
            soe_bsa::Archive::open(&data.join("Skyrim - Misc.bsa")).expect("archive opens");
        let names: Vec<String> = archive
            .files()
            .iter()
            .filter(|n| n.to_lowercase().ends_with(".pex"))
            .cloned()
            .collect();
        for name in names {
            let bytes = archive.read(&name).expect("file reads");
            scripts.push((name, bytes));
        }
    } else {
        eprintln!("SKYRIM_DATA and SOE_PEX_DIR not set, skipping the real-file test");
        return;
    }
    if scripts.is_empty() {
        eprintln!("no pex scripts found, skipping");
        return;
    }
    let (mut ok, mut failed, mut functions) = (0usize, Vec::new(), 0usize);
    let mut vm = Vm::new();
    // Call sites by target, to rank the natives the vanilla scripts lean on.
    let mut statics: std::collections::HashMap<String, usize> =
        std::collections::HashMap::default();
    let mut methods: std::collections::HashMap<String, usize> =
        std::collections::HashMap::default();
    let mut native_decls: std::collections::HashMap<String, usize> =
        std::collections::HashMap::default();
    for (name, bytes) in scripts {
        match pex::parse(&bytes) {
            Ok(s) => {
                ok += 1;
                for o in &s.objects {
                    for st in &o.states {
                        for f in &st.functions {
                            functions += 1;
                            if f.is_native() {
                                *native_decls.entry(o.name.to_ascii_lowercase()).or_default() += 1;
                            }
                            for ins in &f.code {
                                match ins.op {
                                    CallStatic => {
                                        let key = format!(
                                            "{}.{}",
                                            ins.args[0].as_ident().unwrap_or("?"),
                                            ins.args[1].as_ident().unwrap_or("?")
                                        );
                                        *statics.entry(key.to_ascii_lowercase()).or_default() += 1;
                                    }
                                    CallMethod => {
                                        let key = ins.args[0]
                                            .as_ident()
                                            .unwrap_or("?")
                                            .to_ascii_lowercase();
                                        *methods.entry(key).or_default() += 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                vm.load(s);
            }
            Err(err) => failed.push(format!("{name}: {err}")),
        }
    }
    eprintln!(
        "parsed {ok} scripts with {functions} functions, {} failed",
        failed.len()
    );
    for f in &failed {
        eprintln!("  {f}");
    }
    let mut decl: Vec<_> = native_decls.into_iter().collect();
    decl.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    eprintln!(
        "native declarations: {} across {} classes",
        decl.iter().map(|d| d.1).sum::<usize>(),
        decl.len()
    );
    for (c, n) in decl.iter().take(25) {
        eprintln!("  decl {n:5} {c}");
    }
    let mut st: Vec<_> = statics.into_iter().collect();
    st.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    eprintln!("static call sites: {} distinct targets", st.len());
    for (k, n) in st.iter().take(40) {
        eprintln!("  static {n:6} {k}");
    }
    let mut me: Vec<_> = methods.into_iter().collect();
    me.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    eprintln!("method call sites: {} distinct names", me.len());
    for (k, n) in me.iter().take(60) {
        eprintln!("  method {n:6} {k}");
    }
    assert!(ok > 0, "no scripts parsed");
    assert!(failed.is_empty());
    assert!(vm.has_script("Actor") && vm.has_script("ObjectReference") && vm.has_script("Form"));
}

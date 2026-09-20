// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{Script, UserFlag};
use soe_papyrus::{Opcode::*, Value, Vm};

fn conditional(name: &str, flags: u32) -> Script {
    let mut script = ScriptBuilder::new(name, "")
        .auto_property("Stage", "Int", i(1))
        .variable("Private", "Int", i(9))
        .function(
            FunctionBuilder::new("Change", "None")
                .ins(Assign, vec![ident("::Stage_var"), i(7)])
                .build(),
        )
        .build();
    script.user_flags.push(UserFlag {
        name: "Conditional".into(),
        bit: 7,
    });
    script.objects[0].user_flags = flags;
    script.objects[0].variables[0].user_flags = 1 << 7;
    script
}

#[test]
fn conditional_lookup_uses_declared_flag_bit_and_current_inherited_state() {
    let mut vm = Vm::new();
    vm.load(conditional("Parent", 1 << 7));
    vm.load(ScriptBuilder::new("Child", "Parent").build());
    let object = vm.create_object("quest");
    vm.attach(object, "Child").unwrap();
    assert_eq!(vm.conditional_vars(object, "::STAGE_VAR"), [Value::Int(1)]);
    assert!(vm.conditional_vars(object, "Private").is_empty());
    assert!(vm.conditional_vars(object, "Absent").is_empty());
    vm.call(object, "Change", &[]).unwrap();
    assert!(vm.run_until_idle(16));
    assert_eq!(vm.conditional_vars(object, "::Stage_var"), [Value::Int(7)]);
    let state = vm.save_state().unwrap();
    vm.set_var(object, "Child", "::Stage_var", Value::Int(0))
        .unwrap();
    vm.load_state(&state).unwrap();
    assert_eq!(vm.conditional_vars(object, "::Stage_var"), [Value::Int(7)]);
}

#[test]
fn nonconditional_scripts_are_hidden_and_duplicate_names_remain_ambiguous() {
    let mut vm = Vm::new();
    vm.load(conditional("Hidden", 0));
    vm.load(conditional("First", 1 << 7));
    vm.load(conditional("Second", 1 << 7));
    let object = vm.create_object("quest");
    vm.attach(object, "Hidden").unwrap();
    assert!(vm.conditional_vars(object, "::Stage_var").is_empty());
    vm.attach(object, "First").unwrap();
    vm.attach(object, "Second").unwrap();
    assert_eq!(
        vm.conditional_vars(object, "::Stage_var"),
        [Value::Int(1), Value::Int(1)]
    );
}

#[test]
fn conditional_shadow_reads_the_child_without_overwriting_its_parent() {
    let mut vm = Vm::new();
    vm.load(conditional("Parent", 1 << 7));
    let mut child = ScriptBuilder::new("Child", "Parent")
        .variable("::Stage_var", "Int", i(20))
        .build();
    child.user_flags.push(UserFlag {
        name: "Conditional".into(),
        bit: 7,
    });
    child.objects[0].user_flags = 1 << 7;
    child.objects[0].variables[0].user_flags = 1 << 7;
    vm.load(child);
    let object = vm.create_object("quest");
    vm.attach(object, "Child").unwrap();
    vm.set_var(object, "Parent", "::Stage_var", Value::Int(11))
        .unwrap();
    assert_eq!(vm.conditional_vars(object, "::Stage_var"), [Value::Int(20)]);
    assert_eq!(
        vm.get_var(object, "Parent", "::Stage_var"),
        Some(Value::Int(11))
    );
    let saved = vm.save_state().unwrap();
    vm.set_var(object, "Child", "::Stage_var", Value::Int(99))
        .unwrap();
    vm.load_state(&saved).unwrap();
    assert_eq!(vm.conditional_vars(object, "::Stage_var"), [Value::Int(20)]);
    assert_eq!(
        vm.get_var(object, "Parent", "::Stage_var"),
        Some(Value::Int(11))
    );
}

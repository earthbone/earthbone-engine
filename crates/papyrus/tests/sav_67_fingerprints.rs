// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use soe_papyrus::asm::{i, write, ScriptBuilder};
use soe_papyrus::Vm;

fn script(value: i32) -> Vec<u8> {
    write(
        &ScriptBuilder::new("Example", "")
            .variable("Count", "Int", i(value))
            .build(),
    )
}

#[test]
fn sav_67_changed_loaded_pex_definition_changes_source_fingerprint() {
    let mut first = Vm::new();
    let mut second = Vm::new();
    first.load_bytes(&script(1)).unwrap();
    second.load_bytes(&script(2)).unwrap();
    let original = first.loaded_script_fingerprints();
    let changed = second.loaded_script_fingerprints();
    assert_eq!(original.len(), 1);
    assert!(original[0].1.is_some());
    assert_ne!(original, changed);
}

#[test]
fn sav_67_direct_programmatic_script_load_reports_unknown_source() {
    let mut vm = Vm::new();
    vm.load(ScriptBuilder::new("Example", "").build());
    assert_eq!(
        vm.loaded_script_fingerprints(),
        vec![("Example".into(), None)]
    );
}

#[test]
fn sav_67_script_fingerprint_inventory_is_sorted_and_replacement_is_current() {
    let mut vm = Vm::new();
    vm.load(ScriptBuilder::new("Zulu", "").build());
    vm.load_bytes(&script(1)).unwrap();
    vm.load(ScriptBuilder::new("alpha", "").build());
    let names: Vec<_> = vm
        .loaded_script_fingerprints()
        .into_iter()
        .map(|entry| entry.0)
        .collect();
    assert_eq!(names, ["alpha", "Example", "Zulu"]);
    vm.load(ScriptBuilder::new("EXAMPLE", "").build());
    assert_eq!(vm.loaded_script_fingerprints()[1], ("EXAMPLE".into(), None));
}

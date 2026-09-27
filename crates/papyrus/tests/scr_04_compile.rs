// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_papyrus::{pex, psc, Value, Vm};

fn flags(script: &pex::Script) -> Vec<(String, u8)> {
    let mut flags: Vec<_> = script
        .user_flags
        .iter()
        .map(|flag| (flag.name.clone(), flag.bit))
        .collect();
    flags.sort();
    flags
}

#[test]
fn scr_04_script_header_and_return_compile_to_pex() {
    let source = "\n; comment\nScriptName Example extends Form Hidden\nInt Function Answer()\n  Return 42\nEndFunction\n";
    let bytes = psc::compile("Example.psc", source).unwrap();
    let compiled = pex::parse(&bytes).unwrap();
    let object = compiled.object();
    assert_eq!(object.name, "Example");
    assert_eq!(object.parent, "Form");
    assert_eq!(object.user_flags, 1);
    assert_eq!(
        object.states[0].functions[2].code[0].args,
        vec![pex::Operand::Int(42)]
    );
}

#[test]
fn scr_04_compiled_psc_scenario_runs_in_vm() {
    let source = "ScriptName Example\nInt Function Answer()\nReturn 42\nEndFunction\n";
    let bytes = psc::compile("Example.psc", source).unwrap();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    let object = vm.create_object("scenario");
    vm.attach(object, "Example").unwrap();
    let thread = vm.call(object, "Answer", &[]).unwrap();
    assert!(vm.run_until_idle(100), "compiled scenario should finish");
    assert_eq!(vm.thread_result(thread), Some(Value::Int(42)));
}

#[test]
fn scr_04_invalid_or_unhandled_source_is_rejected() {
    assert!(matches!(
        psc::compile("Other.psc", "ScriptName Example"),
        Err(psc::CompileError::Filename { .. })
    ));
    assert!(matches!(
        psc::compile("Example.txt", "ScriptName Example"),
        Err(psc::CompileError::Filename { .. })
    ));
    assert!(matches!(
        psc::compile("Function.psc", "ScriptName Function"),
        Err(psc::CompileError::Header { line: 1 })
    ));
    assert!(matches!(
        psc::compile(
            "Example.psc",
            "ScriptName Example\nInt Function F()\n  x = 1\nEndFunction"
        ),
        Err(psc::CompileError::Syntax { line: 3, .. })
    ));
}

#[test]
fn scr_04_vanilla_activator_matches_creation_kit_pex() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/skse64/scripts/vanilla/Activator.psc";
    let source = std::fs::read_to_string(source).expect("vanilla source is available");
    let ours = pex::parse(&psc::compile("Activator.psc", &source).unwrap()).unwrap();
    let archive = soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Misc.bsa")).unwrap();
    let shipped = archive.read("scripts\\activator.pex").unwrap();
    let theirs = pex::parse(&shipped).unwrap();
    assert_eq!(flags(&ours), flags(&theirs));
    assert_eq!(ours.header.game_id, theirs.header.game_id);
    assert_eq!(
        (ours.header.major, ours.header.minor),
        (theirs.header.major, theirs.header.minor)
    );
    assert_eq!(ours.objects, theirs.objects);
}

#[test]
fn scr_04_vanilla_armor_native_matches_creation_kit_pex() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/skse64/scripts/vanilla/Armor.psc";
    let source = std::fs::read_to_string(source).expect("vanilla source is available");
    let ours = pex::parse(&psc::compile("Armor.psc", &source).unwrap()).unwrap();
    let archive = soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Misc.bsa")).unwrap();
    let shipped = archive.read("scripts\\armor.pex").unwrap();
    let theirs = pex::parse(&shipped).unwrap();
    assert_eq!(flags(&ours), flags(&theirs));
    assert_eq!(ours.header.game_id, theirs.header.game_id);
    assert_eq!(
        (ours.header.major, ours.header.minor),
        (theirs.header.major, theirs.header.minor)
    );
    assert_eq!(ours.objects, theirs.objects);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

use soe_commonlib_shim::conditions::{
    ComparisonValue, ConditionContext, ConditionError, ConditionRuntime, CONDITIONITEMOBJECT,
    CONDITION_ITEM_DATA,
};
use soe_commonlib_shim::{form::FormId, refr::ObjectRefHandle};

fn evaluate_request(game: &mut soe_game::Game, line: &str) -> (u8, u32, bool) {
    let fields: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(fields.len(), 17, "condition bridge request field count");
    let number = |index: usize| fields[index].parse::<u64>().unwrap();
    let flags = number(0) as u8;
    let comparison = if number(1) != 0 {
        ComparisonValue::Global(FormId(number(3) as u32))
    } else {
        ComparisonValue::Number(fields[2].parse::<f32>().unwrap())
    };
    let object = match number(10) {
        0 => CONDITIONITEMOBJECT::SelfRef,
        1 => CONDITIONITEMOBJECT::Target,
        2 => CONDITIONITEMOBJECT::Ref,
        3 => CONDITIONITEMOBJECT::CombatTarget,
        4 => CONDITIONITEMOBJECT::LinkedRef,
        5 => CONDITIONITEMOBJECT::QuestAlias,
        6 => CONDITIONITEMOBJECT::PackData,
        7 => CONDITIONITEMOBJECT::EventData,
        8 => CONDITIONITEMOBJECT::CommandTarget,
        _ => return (5, 0, false),
    };
    let item = CONDITION_ITEM_DATA {
        flags,
        comparison,
        function: number(4) as u16,
        parameters: [number(5) as u32, number(6) as u32],
        run_on: number(7) as u32,
        reference: FormId(number(8) as u32),
        parameter_3: fields[9].parse().unwrap(),
        object,
    };
    let context = ConditionContext {
        owner: FormId(number(11) as u32),
        subject: ObjectRefHandle(number(12) as u32),
        target: (number(14) != 0).then_some(ObjectRefHandle(number(13) as u32)),
        quest: (number(16) != 0).then_some(FormId(number(15) as u32)),
    };
    match game.evaluate_item(&item, context) {
        Ok(result) => (0, 0, result),
        Err(ConditionError::UnsupportedFunction(function)) => (1, function as u32, false),
        Err(ConditionError::UnsupportedTarget(target)) => (2, target, false),
        Err(ConditionError::UnsupportedHook(hook)) => (3, hook as u32, false),
        Err(ConditionError::UnavailableState) => (4, 0, false),
        Err(ConditionError::InvalidCondition) => (5, 0, false),
    }
}

#[test]
fn shm_17_cpp_condition_calls_live_game_and_reports_unsupported_operations() {
    let Some(mut game) = crate::common::installed::game(&["Skyrim.esm"]) else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shim = root.join("../../commonlib-shim");
    let scratch = std::env::temp_dir().join(format!("soe-shm-17-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let binary = scratch.join("condition_bridge");
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let output = Command::new(compiler)
        .arg("-std=c++20")
        .arg("-Werror")
        .arg("-I")
        .arg(shim.join("include"))
        .arg(shim.join("tests/fixtures/shm_17_condition_bridge.cpp"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++ compiler is required for the SHM-17 bridge test");
    assert!(
        output.status.success(),
        "C++ bridge failed to compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("compiled C++ bridge starts");
    let mut input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut requests = BufReader::new(stdout);
    for call in 0..2 {
        let mut request = String::new();
        requests.read_line(&mut request).unwrap();
        assert!(!request.is_empty(), "C++ bridge call {call} was not sent");
        let (code, detail, result) = evaluate_request(&mut game, &request);
        writeln!(input, "{code} {detail} {}", u8::from(result)).unwrap();
        input.flush().unwrap();
    }
    drop(input);
    assert!(
        child.wait().unwrap().success(),
        "C++ bridge rejected runtime result"
    );
    std::fs::remove_dir_all(scratch).unwrap();
}

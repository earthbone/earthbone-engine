// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

const CPP_BRIDGE_PROBE: &str = r#"
#include <RE/Conditions.h>
#include <iostream>
#include <sstream>

int main() {
    std::istringstream input("0 0 1\n0 0 0\n");
    std::ostringstream output;
    RE::ConditionRuntimeBridge bridge(input, output);
    RE::CONDITION_ITEM_DATA item{};
    item.flags = 0xE5;
    item.comparison.number = 0.12345679F;
    item.function = 14;
    item.parameters[0] = 0xFEDCBA98;
    item.parameters[1] = 0x87654321;
    item.run_on = 2;
    item.reference.value = 0xABCDEF12;
    item.parameter_3 = -2147483647;
    item.object = RE::CONDITIONITEMOBJECT::kSelf;
    RE::ConditionContext context{};
    context.owner.value = 0x10203040;
    context.subject.value = 0x50607080;
    context.target.value = 0x90ABCDEF;
    context.has_target = true;
    context.quest.value = 0xA1B2C3D4;
    context.has_quest = true;
    bool result = false;
    auto error = bridge.evaluate_item(item, context, result);
    if (error.code != RE::ConditionErrorCode::None || !result) return 1;

    item.comparison.is_global = true;
    item.comparison.global.value = 0xF1234567;
    error = bridge.evaluate_item(item, context, result);
    if (error.code != RE::ConditionErrorCode::None || result) return 2;
    std::cout << output.str();
}
"#;

fn run_cpp_probe(source: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shim = root.join("../commonlib-shim");
    let scratch = root
        .join("../../target")
        .join(format!("soe-shm-17-adversarial-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source_path = scratch.join("probe.cpp");
    let binary = scratch.join("probe");
    fs::write(&source_path, source).unwrap();
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let compile = Command::new(compiler)
        .arg("-std=c++20")
        .arg("-Werror")
        .arg("-I")
        .arg(shim.join("include"))
        .arg(&source_path)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++ compiler is required for the SHM-17 bridge test");
    assert!(
        compile.status.success(),
        "C++ bridge probe did not compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(binary)
        .stdin(Stdio::null())
        .output()
        .expect("C++ bridge probe starts");
    assert!(
        output.status.success(),
        "C++ bridge probe rejected its replies"
    );
    fs::remove_dir_all(scratch).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn shm_17_bridge_preserves_ctda_float_and_context_bits() {
    let output = run_cpp_probe(CPP_BRIDGE_PROBE);
    let lines: Vec<Vec<&str>> = output
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].len(), 17);
    assert_eq!(lines[1].len(), 17);
    assert_eq!(lines[0][0], "229");
    assert_eq!(lines[0][4], "14");
    assert_eq!(lines[0][5], "4275878552");
    assert_eq!(lines[0][6], "2271560481");
    assert_eq!(lines[0][8], "2882400018");
    assert_eq!(lines[0][9], "-2147483647");
    assert_eq!(lines[0][11], "270544960");
    assert_eq!(lines[0][12], "1348497536");
    assert_eq!(lines[0][13], "2427178479");
    assert_eq!(lines[0][14], "1");
    assert_eq!(lines[0][15], "2712847316");
    assert_eq!(lines[0][16], "1");
    assert_eq!(lines[1][1], "1");
    assert_eq!(lines[1][3], "4045620583");
    let expected = 0.12345679_f32;
    let transmitted = lines[0][2].parse::<f32>().unwrap();
    assert_eq!(
        transmitted.to_bits(),
        expected.to_bits(),
        "CommonLib CTDA comparison float changed in bridge transport"
    );
}

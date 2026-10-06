// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{fs, process::Command};

#[test]
fn shm_17_condition_bridge_rejects_unknown_error_codes() {
    let include =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    let scratch =
        std::env::temp_dir().join(format!("soe-shm17-code-adversarial-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("condition.cpp");
    let binary = scratch.join("condition");
    fs::write(&source, r#"
#include <RE/Conditions.h>
#include <sstream>
int main() {
  std::istringstream input("255 0 1\n"); std::ostringstream output;
  RE::ConditionRuntimeBridge bridge(input, output);
  RE::CONDITION_ITEM_DATA item{}; RE::ConditionContext context{}; bool result = false;
  const auto error = bridge.evaluate_item(item, context, result);
  return static_cast<unsigned>(error.code) <= static_cast<unsigned>(RE::ConditionErrorCode::InvalidCondition) ? 0 : 1;
}
"#).unwrap();
    let compile = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(binary).status().unwrap();
    let _ = fs::remove_dir_all(scratch);
    assert!(
        run.success(),
        "unknown bridge response codes must fail closed"
    );
}

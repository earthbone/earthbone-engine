// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_15_cpp_world_contract_compiles_and_runs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-15-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("world.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <type_traits>
static_assert(std::is_base_of_v<RE::TES, RE::CallbackTES>);
static_assert(std::is_standard_layout_v<RE::RuntimeStateCallbacks>);
static_assert(RE::SIT_SLEEP_STATE::kIsSleeping == static_cast<RE::SIT_SLEEP_STATE>(7));
int main() { return 0; }
",
    )
    .unwrap();
    let binary = scratch.join("world-contract");
    let compiled = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-15");
    assert!(
        compiled.status.success(),
        "C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let ran = Command::new(&binary).output().unwrap();
    assert!(ran.status.success(), "C++ fixture did not pass");
}

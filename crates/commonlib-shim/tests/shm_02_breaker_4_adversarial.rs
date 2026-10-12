// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn compile(source: &Path, output: &Path) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(root.join("include"))
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .expect("compile native registry contract fixture")
}

#[test]
fn shm_02_same_function_name_in_different_papyrus_classes_is_not_duplicate() {
    let dir = std::env::temp_dir().join(format!("soe-shm-02-class-key-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let cpp = dir.join("registry.cpp");
    let binary = dir.join("registry");
    fs::write(
        &cpp,
        r#"
#include <RE/VirtualMachine.h>
#include <cassert>
int main()
{
    RE::NativeRegistry registry;
    assert(registry.register_native({"Form", "GetName", false}));
    assert(registry.register_native({"ObjectReference", "GetName", false}));
    assert(registry.len() == 2);
    assert(registry.duplicates().empty());
    assert(registry.get("Form", "GetName"));
    assert(registry.get("ObjectReference", "GetName"));
}
"#,
    )
    .unwrap();
    let built = compile(&cpp, &binary);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(binary).output().unwrap();
    assert!(run.status.success());
    fs::remove_dir_all(dir).unwrap();
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{fs, process::Command};

#[test]
fn shm_15_cpp_setting_rejects_integer_values_outside_int32_range() {
    let include =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    let scratch = std::env::temp_dir().join(format!(
        "soe-shm15-range-adversarial-{}",
        std::process::id()
    ));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("setting.cpp");
    let binary = scratch.join("setting");
    fs::write(
        &source,
        r#"
#include <RE/WorldPlayer.h>
#include <limits>
bool setting(void*, const char*, RE::RuntimeSettingRecord* out) {
  out->kind = 1; out->integer = std::numeric_limits<std::int64_t>::max(); return true;
}
float sky(void*) { return 0.0F; }
bool reanimate(void*, std::uint64_t, RE::RuntimeReanimateRecord*) { return false; }
void sit_sleep(void*, std::uint32_t, RE::RuntimeErrorRecord*) {}
void hook(void*, const char*, RE::RuntimeErrorRecord*) {}
int main() {
  const RE::RuntimeStateCallbacks cb{setting, sky, reanimate, sit_sleep, hook};
  return RE::CallbackTES(nullptr, cb).setting("iTest") ? 1 : 0;
}
"#,
    )
    .unwrap();
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
        "out-of-range setting must not silently narrow to int32"
    );
}

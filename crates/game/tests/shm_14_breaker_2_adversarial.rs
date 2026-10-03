// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{fs, process::Command};

#[test]
fn shm_14_cpp_player_does_not_expose_zero_as_a_valid_object_handle() {
    let include =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    let scratch =
        std::env::temp_dir().join(format!("soe-shm14-zero-adversarial-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("handle.cpp");
    let binary = scratch.join("handle");
    fs::write(&source, r"
#include <RE/WorldPlayer.h>
bool player(void*, RE::FormKey* out) { *out = {0, 0x14}; return true; }
std::size_t count(void*) { return 0; }
bool actor(void*, std::size_t, RE::FormKey*) { return false; }
bool position(void*, RE::FormKey, RE::Position*) { return false; }
bool value(void*, RE::FormKey, const char*, float*) { return false; }
bool dead(void*, RE::FormKey, bool*) { return false; }
std::uint32_t runtime_id(void*, RE::FormKey) { return 0; }
int main() {
  const RE::PlayerWorldStateCallbacks cb{nullptr, player, count, actor, position, value, dead, runtime_id};
  RE::CallbackPlayerWorldState world(cb);
  RE::PlayerCharacter player_view(world);
  return player_view.handle().has_value() ? 1 : 0;
}
").unwrap();
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
        "runtime FormID zero must map to an empty ObjectRefHandle"
    );
}

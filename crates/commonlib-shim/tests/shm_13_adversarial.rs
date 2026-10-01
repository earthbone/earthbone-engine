// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{fs, path::PathBuf, process::Command};

#[test]
fn map_marker_bit_updates_preserve_other_flags_and_are_idempotent() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-13-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("flags.cpp");
    fs::write(
        &source,
        r"
#include <RE/Magic.h>
#include <cassert>
int main() {
    RE::MapMarkerData marker{0x84};
    marker.set_visible(true);
    assert(marker.flags == 0x85 && marker.visible());
    marker.set_can_travel_to(true);
    assert(marker.flags == 0x87 && marker.can_travel_to());
    marker.set_visible(false);
    marker.set_visible(false);
    assert(marker.flags == 0x86 && !marker.visible());
}
",
    )
    .unwrap();
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("flags"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(scratch.join("flags"))
        .status()
        .unwrap()
        .success());
    fs::remove_dir_all(scratch).unwrap();
}

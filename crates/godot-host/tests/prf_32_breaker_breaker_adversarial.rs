// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

#[path = "common/prf_32.rs"]
mod support;

#[test]
fn prf_32_installed_repair_accepts_the_exact_44_vertex_input() {
    let Some(output) = support::installed_collision() else {
        return;
    };
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && log.contains("RESULT: PASS"),
        "the installed collision did not build and support contact:\n{log}"
    );
    assert!(
        !log.contains("Failed to build Jolt Physics convex polygon shape"),
        "Jolt rejected a convex collision shape:\n{log}"
    );
    let repaired_44 = log.lines().any(|line| {
        line.contains("repaired convex collision hull for mesh node")
            && line.contains("input_vertices=44)")
    });
    assert!(
        repaired_44,
        "the installed input with 44 vertices was not repaired:\n{log}"
    );
}

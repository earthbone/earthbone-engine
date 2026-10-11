// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

use std::collections::HashSet;

#[path = "common/prf_32.rs"]
mod support;

#[test]
fn prf_32_bannered_mare_repairs_are_valid_and_diagnostics_identify_each_source_once() {
    let Some(output) = support::installed_collision() else {
        return;
    };
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        output.status.success()
            && log.contains("static bodies built:")
            && log.contains("RESULT: PASS"),
        "the Bannered Mare collision did not support a resting body:\n{log}"
    );
    assert!(
        !log.contains("Failed to build Jolt Physics convex polygon shape"),
        "Jolt rejected a host-built convex shape:\n{log}"
    );

    let repairs: Vec<_> = log
        .lines()
        .filter(|line| line.contains("repaired convex collision hull for mesh node"))
        .collect();
    assert!(
        !repairs.is_empty(),
        "the installed cell did not exercise a repaired convex hull:\n{log}"
    );
    assert!(
        repairs
            .iter()
            .any(|line| line.contains("input_vertices=44")),
        "the installed cell did not repair the logged 44-vertex hull:\n{log}"
    );
    let mut sources = HashSet::new();
    for line in repairs {
        let source = match line.split_once("mesh node ") {
            Some((_, source)) => source.trim(),
            None => panic!("repair diagnostic names its source: {line}"),
        };
        assert!(
            source.contains("meshes/"),
            "repair diagnostic omits the mesh path: {source}"
        );
        assert!(
            sources.insert(source.to_owned()),
            "source mesh was reported more than once: {source}"
        );
    }
}

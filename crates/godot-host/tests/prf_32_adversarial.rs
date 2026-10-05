// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

#[test]
fn bannered_mare_hull_repairs_are_built_and_reported_once() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        panic!("SOE_GODOT_BIN is required");
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        panic!("SKYRIM_DATA is required");
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = match Command::new(godot)
        .args(["--headless", "--path"])
        .arg(&project)
        .arg("--script")
        .arg(project.join("physics_test.gd"))
        .env("SKYRIM_DATA", data)
        .env("GODOT_SILENCE_ROOT_WARNING", "1")
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("Godot process starts: {error}"),
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
        "the installed collision did not build and support a resting body:\n{log}"
    );
    assert!(
        !log.contains("Failed to build Jolt Physics convex polygon shape"),
        "Jolt rejected a convex collision shape:\n{log}"
    );

    let repairs: Vec<_> = log
        .lines()
        .filter(|line| line.contains("repaired convex collision hull for mesh node"))
        .collect();
    assert!(
        !repairs.is_empty(),
        "the installed cell did not exercise a repaired convex hull:\n{log}"
    );
    let mut sources = HashSet::new();
    for line in repairs {
        let source = match line.split_once("mesh node ") {
            Some((_, source)) => source.trim(),
            None => panic!("repair log includes a source mesh node: {line}"),
        };
        assert!(
            sources.insert(source.to_owned()),
            "source mesh was reported more than once: {source}"
        );
    }
}

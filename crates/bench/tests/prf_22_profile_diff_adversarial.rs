// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

fn run(before: &serde_json::Value, after: &serde_json::Value) -> String {
    let test_name = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_owned();
    let directory =
        std::env::temp_dir().join(format!("prf-22-breaker-{}-{test_name}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let before_path = directory.join("before.json");
    let after_path = directory.join("after.json");
    std::fs::write(&before_path, before.to_string()).unwrap();
    std::fs::write(&after_path, after.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before_path)
        .arg(&after_path)
        .output()
        .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn prf_22_legacy_function_names_group_and_keep_game_host_systems_distinct() {
    let before = serde_json::json!({"traceEvents": [
        {"ph":"C","cat":"script","name":"ScriptA.First.execute","args":{"self_ns":100,"calls":1}},
        {"ph":"C","cat":"script","name":"ScriptA.Second.execute","args":{"self_ns":200,"calls":2}},
        {"ph":"C","cat":"system","name":"papyrus_vm.game","args":{"self_ns":10,"calls":1,"phase":"game","entity":"papyrus_vm"}},
        {"ph":"C","cat":"system","name":"papyrus_vm.host","args":{"self_ns":20,"calls":1,"phase":"host","entity":"papyrus_vm"}}
    ]});
    let after = serde_json::json!({"traceEvents": [
        {"ph":"C","cat":"script","name":"ScriptA.First.execute","args":{"self_ns":150,"calls":1}},
        {"ph":"C","cat":"script","name":"ScriptA.Second.execute","args":{"self_ns":250,"calls":2}},
        {"ph":"C","cat":"system","name":"papyrus_vm.game","args":{"self_ns":30,"calls":2,"phase":"game","entity":"papyrus_vm"}},
        {"ph":"C","cat":"system","name":"papyrus_vm.host","args":{"self_ns":40,"calls":2,"phase":"host","entity":"papyrus_vm"}}
    ]});
    let report = run(&before, &after);
    assert!(
        report.contains("\"ScriptA\" 300 400 +100 +33.33%"),
        "{report}"
    );
    assert!(
        report.contains("\"game.papyrus_vm\" 10 30 +20 +200.00%"),
        "{report}"
    );
    assert!(
        report.contains("\"host.papyrus_vm\" 20 40 +20 +100.00%"),
        "{report}"
    );
}

#[test]
fn prf_22_diff_ranks_the_largest_ten_signed_changes_with_stable_ties() {
    let events =
        |scale: u64| {
            serde_json::Value::Array((0..12).map(|index| serde_json::json!({
            "ph":"C", "cat":"actor", "name":format!("Actor{index}.tick"),
            "args":{"entity":format!("Actor{index}"),"self_ns":index * scale,"calls":1}
        })).collect())
        };
    let report = run(
        &serde_json::json!({"traceEvents": events(1)}),
        &serde_json::json!({"traceEvents": events(2)}),
    );
    let actors = report.split("\nactor:\n").nth(1).unwrap();
    let self_time = actors.split("\n  calls:").next().unwrap();
    assert!(
        self_time.contains("\"Actor11\" 11 22 +11 +100.00%"),
        "{self_time}"
    );
    assert!(!self_time.contains("Actor0\""), "{self_time}");
    assert_eq!(
        self_time
            .lines()
            .filter(|line| line.starts_with("    "))
            .count(),
        10
    );
}

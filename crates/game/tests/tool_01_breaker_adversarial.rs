// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    mo2: PathBuf,
    scenarios: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("soe-tool-01-breaker-{}-{stamp}", std::process::id()));
        let game = root.join("game");
        let mo2 = root.join("mo2");
        let scenarios = root.join("scenarios");
        fs::create_dir_all(game.join("Data")).unwrap();
        fs::create_dir_all(mo2.join("profiles/Default")).unwrap();
        fs::write(mo2.join("profiles/Default/plugins.txt"), "").unwrap();
        fs::write(mo2.join("profiles/Default/loadorder.txt"), "").unwrap();
        fs::write(mo2.join("profiles/Default/modlist.txt"), "").unwrap();
        fs::create_dir_all(&scenarios).unwrap();
        Self { root, game, mo2, scenarios }
    }

    fn scenario(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.scenarios.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_soe-validate"))
            .arg(&self.game)
            .arg(&self.mo2)
            .arg("Default")
            .arg(&self.scenarios)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn tool_01_reports_all_nested_scenarios_in_sorted_order_after_a_failure() {
    let fixture = Fixture::new();
    fixture.scenario(
        "z/pass.scenario",
        "# redesign: empty scenario is valid\ntick 0\n",
    );
    fixture.scenario(
        "a/fail.scenario",
        "# preserve: missing trace must fail\nexpect trace \"not emitted\"\n",
    );

    let output = fixture.run();
    let report = stdout(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("Conformance report: 2 scenario(s)"), "{report}");
    assert!(report.contains("FAIL fail [preserve: missing trace must fail]"), "{report}");
    assert!(report.contains("PASS pass [redesign: empty scenario is valid]"), "{report}");
    assert!(report.find("FAIL fail").unwrap() < report.find("PASS pass").unwrap(), "{report}");
    assert!(report.contains("Overall: FAIL"), "{report}");
}

#[test]
fn tool_01_missing_capture_is_a_reported_scenario_failure_and_keeps_its_path() {
    let fixture = Fixture::new();
    let scenario = fixture.scenario(
        "nested/check.scenario",
        "# preserve: captured trace is the oracle\nexpect traces-match captures/missing.log\n",
    );
    let expected_capture = scenario.parent().unwrap().join("captures/missing.log");

    let output = fixture.run();
    let report = stdout(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("FAIL check [preserve: captured trace is the oracle]"), "{report}");
    assert!(report.contains("captures:"), "{report}");
    assert!(report.contains(&expected_capture.display().to_string()), "{report}");
    assert!(report.contains("No such file or directory"), "{report}");
}

#[test]
fn tool_01_rejects_a_directory_without_scenarios() {
    let fixture = Fixture::new();
    fs::write(fixture.scenarios.join("notes.txt"), "not a scenario").unwrap();

    let output = fixture.run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no .scenario files found"));
}

#[test]
fn tool_01_cli_invalid_arguments_have_the_documented_exit_code() {
    let output = Command::new(env!("CARGO_BIN_EXE_soe-validate"))
        .arg("only-one-argument")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: soe-validate"));
}

#[test]
fn tool_01_rejects_a_non_claim_comment_before_a_later_claim() {
    let fixture = Fixture::new();
    fixture.scenario(
        "wrong-claim.scenario",
        "# scenario note\n# preserve: should not override the first comment\ntick 0\n",
    );

    let output = fixture.run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("first comment line must be"));
}

#[test]
fn tool_01_malformed_scenario_is_reported_as_a_runner_error() {
    let fixture = Fixture::new();
    let scenario = fixture.scenario(
        "nested/bad.scenario",
        "# preserve: malformed steps must be rejected\nnot-a-step\n",
    );

    let output = fixture.run();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("validation runner:"), "{error}");
    assert!(error.contains(&scenario.display().to_string()), "{error}");
    assert!(error.contains("unknown step not-a-step"), "{error}");
}

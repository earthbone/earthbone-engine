// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::harness::{self, Check, Scenario, Step};
use soe_game::Game;
use std::path::PathBuf;

#[test]
fn tst_11_vanilla_log_becomes_ordered_scenario_expectations() {
    let log = "[09/20/2026 - 10:00:01AM] first trace\n\n[09/20/2026 - 10:00:02AM] second trace\n";
    let mut scenario = Scenario::default();

    scenario.append_papyrus_log_expectations(log);

    assert_eq!(
        scenario.steps,
        vec![(
            1,
            Step::Expect(Check::TraceSequence(vec![
                "first trace".to_string(),
                "second trace".to_string(),
            ])),
        )]
    );
}

#[test]
fn tst_11_differential_report_lists_every_missing_trace() {
    let log = "[09/20/2026 - 10:00:01AM] first trace\n[09/20/2026 - 10:00:02AM] middle trace\n[09/20/2026 - 10:00:03AM] last trace\n";
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(log);

    let mut game = Game::new(LoadOrder::default());
    game.vm.log.push(0.0, "middle trace");
    game.vm.log.push(0.0, "extra engine trace");
    let report = harness::run(&mut game, &scenario);

    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    let failure = &report.failures[0];
    assert!(
        failure.contains("expected trace 0 \"first trace\""),
        "{failure}"
    );
    assert!(
        failure.contains("expected trace 2 \"last trace\""),
        "{failure}"
    );
    assert_eq!(failure.matches("expected trace ").count(), 2, "{failure}");
}

#[test]
fn tst_11_differential_keeps_all_ordered_matches() {
    let log = "[09/20/2026 - 10:00:01AM] first trace\n[09/20/2026 - 10:00:02AM] second trace\n";
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(log);

    let mut game = Game::new(LoadOrder::default());
    game.vm.log.push(0.0, "engine noise");
    game.vm.log.push(0.0, "first trace");
    game.vm.log.push(0.0, "second trace");
    let report = harness::run(&mut game, &scenario);

    assert!(report.passed(), "{:?}", report.failures);
}

#[test]
fn tst_11_real_vanilla_log_differential_reports_every_trace() {
    let Some(path) = std::env::var_os("SOE_PAPYRUS_LOG").map(PathBuf::from) else {
        return;
    };
    let log = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    let traces = harness::papyrus_log_traces(&log);
    assert!(!traces.is_empty(), "{} has no log lines", path.display());
    let captured_line = log
        .lines()
        .find(|line| line.starts_with('[') && line.contains("] "))
        .unwrap_or_else(|| panic!("{} has no timestamped Papyrus line", path.display()));
    let trace_text = &captured_line[captured_line
        .find("] ")
        .map_or(captured_line.len(), |index| index + 2)..];
    assert!(traces.iter().any(|trace| trace == trace_text));

    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(&log);
    let mut game = Game::new(LoadOrder::default());
    let report = harness::run(&mut game, &scenario);

    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    let failure = &report.failures[0];
    assert_eq!(
        failure.matches("expected trace ").count(),
        traces.len(),
        "the differential report must name every divergence from {}",
        path.display()
    );
}

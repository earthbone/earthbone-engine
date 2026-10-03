// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::harness::{self, Scenario};
use soe_game::Game;

#[test]
fn tst_11_empty_timestamped_trace_is_skipped() {
    let traces = harness::papyrus_log_traces(
        "[09/20/2026 - 10:00:01AM] first\n[09/20/2026 - 10:00:02AM] \n[09/20/2026 - 10:00:03AM] last\n",
    );

    assert_eq!(traces, ["first", "last"]);
}

#[test]
fn tst_11_trace_trailing_spaces_are_preserved() {
    let traces = harness::papyrus_log_traces(
        "[09/20/2026 - 10:00:01AM] trace with meaningful spaces   \n",
    );

    assert_eq!(traces, ["trace with meaningful spaces   "]);
}

#[test]
fn tst_11_only_a_vanilla_timestamp_prefix_is_removed() {
    let traces = harness::papyrus_log_traces("[not a timestamp] trace payload\n");

    assert_eq!(traces, ["[not a timestamp] trace payload"]);
}

#[test]
fn tst_11_differential_reports_each_missing_duplicate_and_later_trace() {
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(
        "[09/20/2026 - 10:00:01AM] first\n[09/20/2026 - 10:00:02AM] repeated\n[09/20/2026 - 10:00:03AM] repeated\n[09/20/2026 - 10:00:04AM] last\n",
    );
    let mut game = Game::new(LoadOrder::default());
    game.vm.log.push(0.0, "first");
    game.vm.log.push(0.0, "repeated");

    let report = harness::run(&mut game, &scenario);

    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    let failure = &report.failures[0];
    assert!(failure.contains("expected trace 2 \"repeated\""), "{failure}");
    assert!(failure.contains("expected trace 3 \"last\""), "{failure}");
    assert_eq!(failure.matches("expected trace ").count(), 2, "{failure}");
}

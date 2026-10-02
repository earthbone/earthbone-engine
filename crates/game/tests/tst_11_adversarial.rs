// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::harness::{self, Scenario};
use soe_game::Game;

#[test]
fn tst_11_duplicate_expected_lines_need_distinct_actual_occurrences() {
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(
        "[09/20/2026 - 10:00:01AM] repeated\n[09/20/2026 - 10:00:02AM] repeated\n",
    );
    let mut game = Game::new(LoadOrder::new());
    game.vm.log.push(0.0, "repeated");

    let report = harness::run(&mut game, &scenario);
    assert!(!report.passed(), "one trace must not satisfy two log lines");
    assert!(report.failures[0].contains("expected trace 1"));
}

#[test]
fn tst_11_whitespace_only_log_lines_are_blank_expectations() {
    let traces = harness::papyrus_log_traces(
        "[09/20/2026 - 10:00:01AM] first\n   \t\n[09/20/2026 - 10:00:02AM] second\n",
    );
    assert_eq!(traces, ["first", "second"]);
}

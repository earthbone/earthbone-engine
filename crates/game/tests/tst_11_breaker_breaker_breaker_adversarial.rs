// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::harness::{self, Check, Scenario, Step};
use soe_game::Game;

#[test]
fn tst_11_impossible_month_is_not_removed_as_a_vanilla_timestamp() {
    let line = "[19/20/2026 - 10:00:01AM] trace payload";
    assert_eq!(harness::papyrus_log_traces(line), [line]);
}

#[test]
fn tst_11_impossible_day_is_not_removed_as_a_vanilla_timestamp() {
    let line = "[09/99/2026 - 10:00:01AM] trace payload";
    assert_eq!(harness::papyrus_log_traces(line), [line]);
}

#[test]
fn tst_11_impossible_clock_is_not_removed_as_a_vanilla_timestamp() {
    let line = "[09/20/2026 - 19:75:99PM] trace payload";
    assert_eq!(harness::papyrus_log_traces(line), [line]);
}

#[test]
fn tst_11_valid_twelve_hour_boundaries_are_removed() {
    let log = "[09/20/2026 - 12:00:00AM] midnight\n[09/20/2026 - 12:59:59PM] noon";
    assert_eq!(harness::papyrus_log_traces(log), ["midnight", "noon"]);
}

#[test]
fn tst_11_duplicate_actual_traces_are_consumed_once_in_order() {
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(
        "[09/20/2026 - 10:00:01AM] same\n[09/20/2026 - 10:00:02AM] same\n",
    );
    let mut game = Game::new(LoadOrder::default());
    game.vm.log.push(0.0, "same");
    let report = harness::run(&mut game, &scenario);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("expected trace 1"));
}

#[test]
fn tst_11_log_import_creates_one_ordered_expectation() {
    let mut scenario = Scenario::default();
    scenario.append_papyrus_log_expectations(
        "[09/20/2026 - 10:00:01AM] first\n[09/20/2026 - 10:00:02AM] second\n",
    );
    assert_eq!(
        scenario.steps,
        vec![(
            1,
            Step::Expect(Check::TraceSequence(vec![
                "first".to_string(),
                "second".to_string()
            ]))
        )]
    );
}

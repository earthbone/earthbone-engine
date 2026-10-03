// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::play::{self, Route, Step};
use soe_game::Game;

#[test]
fn ply_02_breaker_quoted_names_and_trailing_comment_parse() {
    let route = Route::parse("expect-stage \"Quest With Spaces\" 12 # checkpoint").unwrap();
    assert_eq!(
        route.steps,
        vec![(
            1,
            Step::ExpectStage {
                quest: "Quest With Spaces".into(),
                stage: 12,
            }
        )]
    );
}

#[test]
fn ply_02_breaker_failure_reports_step_world_state_and_trace() {
    let mut order = LoadOrder::new();
    order
        .add("Empty.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let route = Route::parse("# fail before action\nactivate MissingReference").unwrap();
    let report = play::run(&mut game, &route);
    assert!(!report.passed());
    let formatted = report.format();
    assert!(formatted.contains("FAIL step 1 line 2"), "{formatted}");
    assert!(formatted.contains("world state:"), "{formatted}");
    assert!(formatted.contains("trace:"), "{formatted}");
    assert!(
        formatted.contains("Activate(\"MissingReference\")"),
        "{formatted}"
    );
    assert!(formatted.contains("player"), "{formatted}");
}

#[test]
fn ply_02_breaker_malformed_quote_reports_source_line() {
    let error = Route::parse("expect-stage MQ101 0\nactivate \"unfinished").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("line 2"), "{message}");
    assert!(message.contains("unclosed quote"), "{message}");
}

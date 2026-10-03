// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use soe_game::harness;

#[test]
fn papyrus_trace_payload_keeps_trailing_spaces_and_tabs() {
    let traces = harness::papyrus_log_traces("[09/20/2026 - 10:00:01AM] trace payload  \t\r\n");

    assert_eq!(traces, ["trace payload  \t"]);
}

#[test]
fn parser_does_not_strip_a_non_timestamp_bracketed_trace_prefix() {
    let traces = harness::papyrus_log_traces("[Papyrus] trace payload\n");

    assert_eq!(traces, ["[Papyrus] trace payload"]);
}

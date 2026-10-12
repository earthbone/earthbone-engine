// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_game::harness;

#[test]
fn tst_11_only_valid_calendar_and_clock_values_are_removed_as_timestamps() {
    let log = concat!(
        "[02/29/2024 - 12:00:00AM] leap day\n",
        "[02/29/2025 - 10:00:01AM] invalid leap day\n",
        "[04/31/2026 - 10:00:02AM] invalid month day\n",
        "[13/01/2026 - 10:00:03AM] invalid month\n",
        "[09/20/2026 - 00:00:04AM] invalid hour\n",
        "[09/20/2026 - 13:00:05PM] invalid hour\n",
        "[09/20/2026 - 10:60:06AM] invalid minute\n",
        "[09/20/2026 - 10:00:60AM] invalid second\n",
    );

    assert_eq!(
        harness::papyrus_log_traces(log),
        [
            "leap day",
            "[02/29/2025 - 10:00:01AM] invalid leap day",
            "[04/31/2026 - 10:00:02AM] invalid month day",
            "[13/01/2026 - 10:00:03AM] invalid month",
            "[09/20/2026 - 00:00:04AM] invalid hour",
            "[09/20/2026 - 13:00:05PM] invalid hour",
            "[09/20/2026 - 10:60:06AM] invalid minute",
            "[09/20/2026 - 10:00:60AM] invalid second",
        ]
    );
}

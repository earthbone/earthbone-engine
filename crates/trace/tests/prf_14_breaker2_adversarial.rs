// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{count, Operation, WorkCounts, WorkFrame};

#[test]
fn prf_14_zero_work_calls_do_not_spend_a_frame_budget() {
    let frame = WorkFrame::begin();
    for operation in Operation::ALL {
        count(operation, 0);
    }
    let counts = frame.counts();
    assert_eq!(counts.0, [0; 10]);
    assert_eq!(counts.check(&WorkCounts::default()), Ok(()));
}

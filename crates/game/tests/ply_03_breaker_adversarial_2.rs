// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_game::play::{Route, Step};

#[test]
fn ply_03_route_advances_gameplay_before_checkpoint_comparison() {
    let route = Route::parse("expect-no-vm-error").unwrap();
    assert!(
        route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::Navigate { .. } | Step::Activate(_) | Step::Talk { .. } | Step::WaitStage { .. }
        )),
        "the checkpoint replay must execute gameplay before comparing save N plus 1"
    );
}

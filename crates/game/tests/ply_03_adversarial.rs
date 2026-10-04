// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_game::play::{Route, Step};

#[test]
fn ply_03_checkpoint_replay_route_performs_gameplay() {
    let route = Route::parse("expect-no-vm-error").unwrap();
    assert!(
        route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::Navigate { .. } | Step::Activate(_) | Step::Talk { .. } | Step::WaitStage { .. }
        )),
        "checkpoint replay needs a gameplay step to advance the source save toward its successor"
    );
}

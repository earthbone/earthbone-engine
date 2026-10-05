// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use std::num::NonZeroUsize;
use std::time::Duration;

#[test]
fn prf_17_worker_configuration_does_not_change_world_save_bytes() {
    let mut serial = Game::new(LoadOrder::new());
    let mut parallel = Game::new(LoadOrder::new());
    let before = parallel.world().save_state();

    parallel
        .world_mut()
        .ai
        .set_planning_workers(NonZeroUsize::new(8).unwrap())
        .unwrap();

    assert_eq!(parallel.world().save_state(), before);
    serial.tick(1.0 / 60.0);
    parallel.tick(1.0 / 60.0);
    assert_eq!(parallel.world().save_state(), serial.world().save_state());
}

#[test]
fn prf_17_idle_worker_metrics_reset_and_have_no_phantom_jobs() {
    let mut game = Game::new(LoadOrder::new());
    game.world_mut()
        .ai
        .set_planning_workers(NonZeroUsize::new(8).unwrap())
        .unwrap();

    for _ in 0..3 {
        game.tick(1.0 / 60.0);
        let world = game.world();
        let metrics = world.ai.planning_workers();
        assert_eq!(metrics.jobs, 0);
        assert!(metrics.main_thread_wait.is_zero());
        assert!(metrics.busy.iter().all(Duration::is_zero));
        assert_eq!(metrics.utilisation(), 0.0);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for PRF-27's public AI measurements.

#[test]
fn prf_27_process_cadence_mean_excludes_the_first_observation() {
    let (mut game, ids) = crate::prf_12_frame::actors(2).unwrap();
    for _ in 0..4 {
        game.tick(0.1);
    }

    let high = &game.world().ai.metrics.levels
        [soe_game::ai::metrics::ProcessLevel::High as usize];
    assert_eq!(high.actors, ids.len() as u64);
    assert_eq!(high.updates, ids.len() as u64);
    assert!(
        (high.mean_interval_seconds - 0.1).abs() < 1e-9,
        "four observations at 0.1 seconds should average three measured intervals, got {}",
        high.mean_interval_seconds
    );
}

#[test]
fn prf_27_completed_path_sample_reports_latency_expansion_and_length() {
    let (mut game, ids) = crate::prf_12_frame::actors(2).unwrap();
    for _ in 0..120 {
        game.tick(1.0 / 60.0);
    }

    let world = game.world();
    let path = &world.ai.path_metrics()[&ids[0]];
    let sample = path.samples.back().expect("completed route sample");
    assert!(!sample.failed);
    assert!(!sample.cancelled);
    assert!(sample.ready >= sample.started);
    assert!(sample.expanded > 0);
    assert!(sample.length > 0.0);
}

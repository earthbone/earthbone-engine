// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial PRF-37 worker boundary checks.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use soe_esm::LoadOrder;
use soe_game::simulation::{Config, Error, PlayerPose, Simulation};
use soe_game::Game;

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "worker did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn prf_37_adversarial_rejects_each_invalid_rate_boundary() {
    for config in [
        Config {
            simulation_hz: 1001,
            ..Config::default()
        },
        Config {
            ai_hz: 0,
            ..Config::default()
        },
        Config {
            scripts_hz: 0,
            ..Config::default()
        },
        Config {
            ai_hz: 61,
            ..Config::default()
        },
        Config {
            scripts_hz: 61,
            ..Config::default()
        },
        Config {
            papyrus_budget: Duration::from_secs(1) + Duration::from_nanos(1),
            ..Config::default()
        },
    ] {
        assert!(matches!(
            Simulation::spawn(config, || unreachable!()),
            Err(Error::Configuration)
        ));
    }
}

#[test]
fn prf_37_adversarial_reports_factory_failure_once_without_a_snapshot() {
    let simulation =
        Simulation::spawn(Config::default(), || Err("bad plugin data".into())).unwrap();
    wait_until(|| simulation.failure().is_some());
    assert!(simulation.poll().is_none());
    assert!(
        simulation.failure().is_none(),
        "failure is a one-shot notification"
    );
}

#[test]
fn prf_37_adversarial_worker_owns_game_creation_and_publishes_ticks() {
    let caller = std::thread::current().id();
    let (sent, received) = mpsc::channel();
    let simulation = Simulation::spawn(
        Config {
            simulation_hz: 100,
            ai_hz: 10,
            scripts_hz: 10,
            ..Config::default()
        },
        move || {
            sent.send(std::thread::current().id()).unwrap();
            Ok(Game::new(LoadOrder::new()))
        },
    )
    .unwrap();
    assert_ne!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        caller
    );
    wait_until(|| {
        simulation
            .poll()
            .is_some_and(|pair| pair.current.sequence > 1)
    });
}

#[test]
fn prf_37_adversarial_pose_mailbox_rejects_nonfinite_rotation_and_position() {
    let (release, wait) = mpsc::channel();
    let simulation = Simulation::spawn(Config::default(), move || {
        wait.recv().unwrap();
        Ok(Game::new(LoadOrder::new()))
    })
    .unwrap();
    let valid = PlayerPose {
        cell: None,
        position: [0.0; 3],
        rotation: [0.0; 3],
        sprinting: false,
    };
    assert!(simulation.set_player_pose(valid));
    assert!(!simulation.set_player_pose(PlayerPose {
        rotation: [f32::INFINITY, 0.0, 0.0],
        ..valid
    }));
    assert!(!simulation.set_player_pose(PlayerPose {
        position: [0.0, f32::NAN, 0.0],
        ..valid
    }));
    release.send(()).unwrap();
}

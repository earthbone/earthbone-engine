// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::player_input::session::Session;
use soe_game::player_input::{Command, Driver};
use soe_game::Game;

#[test]
fn prf_22_zero_seed_wait_session_survives_two_replays_and_keeps_final_checkpoint() {
    let mut recorded_game = Game::new(LoadOrder::new());
    let mut recorder = Driver::default();
    recorder.start_recording(&mut recorded_game, 0).unwrap();
    for _ in 0..61 {
        recorder.push(Command::Wait).unwrap();
        recorder.step(&mut recorded_game).unwrap();
    }
    let session = Session::from_bytes(
        &recorder
            .finish_recording(&recorded_game)
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(session.seed(), 0);
    assert_eq!(
        session
            .hashes()
            .iter()
            .map(|hash| hash.tick)
            .collect::<Vec<_>>(),
        [0, 60, 61]
    );
    for _ in 0..2 {
        let mut replayed = Game::new(LoadOrder::new());
        let mut driver = Driver::default();
        driver.start_replay(&mut replayed, &session).unwrap();
        assert_eq!(driver.pending(), 61);
        while driver.step(&mut replayed).is_some() {
            assert!(
                driver.replay_error().is_none(),
                "{:?}",
                driver.replay_error()
            );
        }
        assert_eq!(driver.pending(), 0);
        assert_eq!(
            replayed.state_hash().parts,
            recorded_game.state_hash().parts
        );
    }
}

#[test]
fn prf_22_failed_replay_preserves_queue_ownership_until_reset() {
    let mut recorded_game = Game::new(LoadOrder::new());
    let mut recorder = Driver::default();
    recorder.start_recording(&mut recorded_game, 7).unwrap();
    recorder.push(Command::Wait).unwrap();
    recorder.step(&mut recorded_game).unwrap();
    let session = recorder.finish_recording(&recorded_game).unwrap();

    let mut replayed = Game::new(LoadOrder::new());
    let mut driver = Driver::default();
    driver.start_replay(&mut replayed, &session).unwrap();
    replayed.world_mut().clock.game_days += 1.0;
    driver.step(&mut replayed).unwrap();
    assert!(driver.replay_error().is_some());
    assert_eq!(driver.pending(), 0);
    assert!(driver.step(&mut replayed).is_none());
}

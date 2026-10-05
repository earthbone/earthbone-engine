// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-22 deterministic session checks over the PLY-01 gameplay fixture.

use soe_esm::GlobalFormId;
use soe_game::combat::Hand;
use soe_game::player_input::session::{Error, Session};
use soe_game::player_input::{Command, Confirmation, Driver};
use soe_papyrus::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode;

fn fixture() -> soe_game::Game {
    let mut game = crate::ply_01_player_input::game();
    let script = ScriptBuilder::new("ReplayRandom", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .local("random", "Int")
                .local("unused", "None")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s("RandomInt"), ident("random"), i(1), i(100)],
                )
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s("Wait"), ident("unused"), f(0.1)],
                )
                .ins(Opcode::Jmp, vec![i(-2)])
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game.vm.call_global("ReplayRandom", "Run", &[]).unwrap();
    game
}

fn commands(frames: usize) -> Vec<Command> {
    (0..frames)
        .map(|frame| match frame % 100 {
            0 => Command::MoveTo {
                target: GlobalFormId {
                    file: 0,
                    object: 0x810,
                },
                offset: [frame as f32, 5.0, 0.0],
                match_rotation: true,
            },
            1 => Command::Activate {
                target: GlobalFormId {
                    file: 0,
                    object: 0x810,
                },
            },
            2 => Command::Activate {
                target: GlobalFormId {
                    file: 0,
                    object: 0xffff,
                },
            },
            3 => Command::Turn {
                rotation: [0.0, 0.0, frame as f32 / 600.0],
            },
            _ => Command::Wait,
        })
        .collect()
}

fn record(frames: usize) -> (Session, soe_game::Game, Vec<soe_game::player_input::Event>) {
    let mut game = fixture();
    let mut driver = Driver::default();
    driver.start_recording(&mut game, 0x1234_5678).unwrap();
    for command in commands(frames) {
        driver.push(command).unwrap();
        driver.step(&mut game).unwrap();
    }
    let session = driver.finish_recording(&game).unwrap();
    (session, game, driver.events().to_vec())
}

#[test]
fn prf_22_records_600_frames_and_replays_twice_with_identical_hashes_and_events() {
    let (session, game, events) = record(600);
    let session = Session::from_bytes(&session.to_bytes()).unwrap();
    assert_eq!(session.seed(), 0x1234_5678);
    assert_eq!(session.commands(), commands(600));
    assert_eq!(
        session
            .hashes()
            .iter()
            .map(|hash| hash.tick)
            .collect::<Vec<_>>(),
        (0..=10).map(|i| i * 60).collect::<Vec<_>>()
    );
    assert_ne!(session.hashes()[0].total, session.hashes()[10].total);
    for _ in 0..2 {
        let mut replay = fixture();
        let mut driver = Driver::default();
        driver.start_replay(&mut replay, &session).unwrap();
        for frame in 1..=600 {
            driver.step(&mut replay).unwrap();
            assert!(
                driver.replay_error().is_none(),
                "frame {frame}: {:?}",
                driver.replay_error()
            );
            if frame % 60 == 0 {
                assert_eq!(
                    replay.state_hash().total,
                    session.hashes()[(frame / 60) as usize].total
                );
            }
        }
        assert_eq!(replay.state_hash().parts, game.state_hash().parts);
        for (actual, expected) in driver.events().iter().zip(&events) {
            assert_eq!(actual, expected, "event frame {}", actual.step);
        }
        assert!(driver.step(&mut replay).is_none());
    }
    if let Some(path) = std::env::var_os("SOE_PRF_22_SESSION") {
        session.write(std::path::Path::new(&path)).unwrap();
    }
}

#[test]
fn prf_22_checkpoint_divergence_names_the_frame_and_stops_replay() {
    let (session, _, _) = record(120);
    let mut replay = fixture();
    let mut driver = Driver::default();
    driver.start_replay(&mut replay, &session).unwrap();
    for frame in 1..=60 {
        if frame == 59 {
            replay.world_mut().clock.game_days += 1.0;
        }
        driver.step(&mut replay).unwrap();
    }
    let error = driver.replay_error().unwrap().to_string();
    assert!(
        error.contains("tick 60") && error.contains("world.clock"),
        "{error}"
    );
    assert_eq!(driver.pending(), 0);
    assert!(driver.step(&mut replay).is_none());
}

#[test]
fn prf_22_final_partial_interval_and_empty_session_are_verified() {
    for frames in [0, 1, 61] {
        let (session, game, _) = record(frames);
        let session = Session::from_bytes(&session.to_bytes()).unwrap();
        assert_eq!(session.hashes().last().unwrap().tick, frames as u64);
        let mut replay = fixture();
        let mut driver = Driver::default();
        driver.start_replay(&mut replay, &session).unwrap();
        while driver.step(&mut replay).is_some() {}
        assert!(
            driver.replay_error().is_none(),
            "{:?}",
            driver.replay_error()
        );
        assert_eq!(replay.state_hash().parts, game.state_hash().parts);
    }
}

#[test]
fn prf_22_session_rejects_truncation_corruption_and_busy_queues() {
    let (session, _, _) = record(61);
    let bytes = session.to_bytes();
    for cut in 0..bytes.len() {
        assert!(Session::from_bytes(&bytes[..cut]).is_err(), "cut {cut}");
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(Session::from_bytes(&corrupt)
        .unwrap_err()
        .to_string()
        .contains("checksum"));
    let mut extra = bytes;
    extra.push(0);
    assert!(Session::from_bytes(&extra).is_err());
    let mut game = fixture();
    let mut driver = Driver::default();
    driver.push(Command::Wait).unwrap();
    assert!(matches!(
        driver.start_recording(&mut game, 1),
        Err(Error::Busy)
    ));
    assert!(matches!(
        driver.start_replay(&mut game, &session),
        Err(Error::Busy)
    ));
}

#[test]
fn prf_22_session_transports_every_input_variant_and_reports_rejected_outcomes() {
    let id = GlobalFormId {
        file: 0,
        object: 0xffff,
    };
    let mut game = fixture();
    let mut driver = Driver::default();
    driver.start_recording(&mut game, 1).unwrap();
    let mut inputs = commands(4);
    inputs.extend([
        Command::Attack {
            target: id,
            weapon: id,
            power: true,
        },
        Command::Cast {
            hand: Hand::Left,
            target: Some(id),
            held: true,
        },
        Command::Cast {
            hand: Hand::Right,
            target: None,
            held: false,
        },
        Command::ChooseTopic { topic: id },
        Command::ConfirmMenu(Confirmation::Dialogue),
        Command::ConfirmMenu(Confirmation::Gift),
        Command::ConfirmMenu(Confirmation::Craft {
            bench: id,
            recipe: id,
        }),
    ]);
    for command in &inputs {
        driver.push(command.clone()).unwrap();
        driver.step(&mut game).unwrap();
    }
    let session = driver.finish_recording(&game).unwrap();
    let decoded = Session::from_bytes(&session.to_bytes()).unwrap();
    assert_eq!(decoded.commands(), inputs);
    let mut replay = fixture();
    let mut driver = Driver::default();
    driver.start_replay(&mut replay, &decoded).unwrap();
    while driver.step(&mut replay).is_some() {}
    assert!(
        driver.replay_error().is_none(),
        "{:?}",
        driver.replay_error()
    );
}

#[test]
fn prf_22_replay_rejects_changed_definitions_before_mutating_world() {
    let (session, _, _) = record(1);
    let mut game = fixture();
    game.load_script(&ScriptBuilder::new("Extra", "").bytes())
        .unwrap();
    let before = game.state_hash();
    assert!(Driver::default().start_replay(&mut game, &session).is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn prf_22_replay_reuses_game_and_restores_native_call_counters() {
    let (session, mut game, events) = record(120);
    let expected = game.state_hash();
    for _ in 0..2 {
        let mut driver = Driver::default();
        driver.start_replay(&mut game, &session).unwrap();
        assert!(matches!(
            driver.push(Command::Wait),
            Err(soe_game::player_input::Error::Replay)
        ));
        while driver.step(&mut game).is_some() {}
        assert!(
            driver.replay_error().is_none(),
            "{:?}",
            driver.replay_error()
        );
        assert_eq!(game.state_hash().parts, expected.parts);
        for (actual, expected) in driver.events().iter().zip(&events) {
            assert_eq!(actual, expected, "frame {}", actual.step);
        }
    }
}

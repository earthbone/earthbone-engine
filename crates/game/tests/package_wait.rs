// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Wait fixtures use vanilla PKC2 ordering: ActualSeconds, then StopMovement.

#[path = "common/legacy.rs"]
mod legacy;

#[path = "common/installed.rs"]
mod installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{AiState, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const DEST: u32 = 0x804;
const ENABLED: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(seconds: f32, stop: bool, repeat: bool, override_seconds: Option<f32>) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let tree = package::close_header(package::build_template(TEMPLATE, "WaitThenTravel"), None);
    let tree = package::with_float_input(tree, 5, seconds);
    let tree = package::finish(package::with_bool_input(tree, 9, stop));
    let mut branch = 2u32.to_le_bytes().to_vec();
    branch.extend_from_slice(&u32::from(repeat).to_le_bytes());
    let tree = tree.zsub(T(b"ANAM"), "Sequence").sub(T(b"PRCB"), branch);
    let tree = package::with_procedure(tree, "Wait", &[5, 9], false);
    plugin.push_top(package::with_procedure(tree, "Travel", &[0], false));
    let enabled = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: ENABLED,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let p = package::with_condition(
        package::build_record(PACK, "TimedRoute", 0, Schedule::default()),
        &enabled,
    );
    let p = package::close_header(p, Some(TEMPLATE));
    let mut p = package::with_location_input(p, 0, Location::NearReference(DEST), 0);
    if let Some(seconds) = override_seconds {
        p = package::with_float_input(p, 5, seconds);
    }
    plugin.push_top(package::finish(p));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    for (tag, form, base, x) in [(*b"ACHR", ACTOR, BASE, 0.0f32), (*b"REFR", DEST, 0, 110.0)] {
        let mut data = vec![0; 24];
        data[..4].copy_from_slice(&x.to_le_bytes());
        plugin.push_top(
            Record::new(T(&tag), form)
                .sub(T(b"NAME"), base.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    plugin.push_top(
        Record::new(T(b"GLOB"), ENABLED)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("Wait.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(seconds: f32, stop: bool, repeat: bool) -> Game {
    let game = Game::new(order(seconds, stop, repeat, None));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn state(game: &Game) -> AiState {
    game.world().actor_ref(id(ACTOR)).unwrap().ai.clone()
}

fn position(game: &Game) -> f32 {
    game.world_mut().reference(id(ACTOR)).unwrap().position[0]
}

#[test]
fn ai_34_wait_uses_non_menu_seconds_without_counting_entry_or_driver_calls_twice() {
    for timescale in [0.0, 20.0, 1000.0] {
        for stop in [false, true] {
            let mut game = game(2.0, stop, false);
            game.world_mut().clock.timescale = timescale;
            game.tick(100.0);
            assert_eq!(
                state(&game).procedure,
                Procedure::Wait {
                    stop_movement: stop
                }
            );
            assert_eq!(state(&game).wait_until, Some(102.0));
            assert_eq!(position(&game), 0.0);
            game.tick_menu_mode(60.0, true);
            assert_eq!(game.world().clock.real_seconds, 100.0);
            game.tick(1.5);
            game.world_mut()
                .ai_tick(1000.0, &mut |_, _| panic!("no new script binding"));
            assert!(state(&game).tree.unwrap().active);
            assert_eq!(position(&game), 0.0);
            game.tick(0.5);
            assert!(!state(&game).tree.unwrap().active);
            assert_eq!(position(&game), 0.0);
            game.tick(1.0);
            assert_eq!(position(&game), 110.0);
            assert!(state(&game).tree.unwrap().finished);
            assert_eq!(state(&game).wait_until, None);
        }
    }
}

#[test]
fn wait_overrides_and_saved_remaining_time_survive_loading() {
    let mut original = Game::new(order(100.0, true, false, Some(2.0)));
    assert!(original.world_mut().drive_actor(id(ACTOR)));
    original.tick(0.25);
    original.tick(0.75);
    assert_eq!(state(&original).wait_until, Some(2.25));
    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order(100.0, true, false, Some(2.0)));
    restored.load_state(&saved).unwrap();
    for dt in [0.5, 0.75, 1.0] {
        original.tick(dt);
        restored.tick(dt);
        assert_eq!(state(&restored), state(&original));
        assert_eq!(restored.world().save_state(), original.world().save_state());
        // CONTEXT: AI evaluation counters are diagnostic state, not save payload.
        assert_eq!(
            restored.world().state_hash().actors,
            original.world().state_hash().actors
        );
    }
    assert_eq!(position(&restored), 110.0);
}

#[test]
fn zero_wait_is_endless_and_cancellation_does_not_complete_it() {
    let mut game = game(0.0, false, false);
    game.tick(1.0);
    game.tick(10000.0);
    assert_eq!(state(&game).wait_until, None);
    assert!(state(&game).tree.unwrap().active);
    assert_eq!(position(&game), 0.0);
    game.world_mut().set_global(id(ENABLED), 0.0);
    game.tick(1.0);
    assert_eq!(state(&game).procedure, Procedure::None);
    assert!(state(&game).completed.is_empty());
    game.tick(10000.0);
    assert_eq!(position(&game), 0.0);
    assert!(state(&game).completed.is_empty());
}

#[test]
fn cancelled_finite_wait_loses_its_deadline_without_success() {
    let mut game = game(2.0, true, false);
    game.tick(1.0);
    game.world_mut().set_global(id(ENABLED), 0.0);
    game.tick(1.0);
    assert_eq!(state(&game).wait_until, None);
    game.tick(10.0);
    assert_eq!(position(&game), 0.0);
    assert!(state(&game).completed.is_empty());
}

#[test]
fn repeated_sequence_starts_a_new_wait_after_its_travel() {
    let mut game = game(1.0, false, true);
    game.tick(0.5);
    game.tick(1.0);
    game.tick(1.0);
    assert_eq!(position(&game), 110.0);
    game.tick(0.5);
    assert_eq!(state(&game).wait_until, Some(4.0));
    game.tick(0.5);
    assert!(state(&game).tree.unwrap().active);
    assert!(state(&game).completed.is_empty());
}

#[test]
fn invalid_wait_durations_are_reported_without_becoming_endless_waits() {
    for duration in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut game = game(duration, false, false);
        game.tick(1.0);
        assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
        assert_eq!(state(&game).wait_until, None);
        game.tick(100.0);
        assert_eq!(position(&game), 0.0);
        assert!(state(&game).completed.is_empty());
    }
}

#[test]
fn wait_deadlines_affect_hashes_and_invalid_or_old_save_tags_are_rejected() {
    use soe_papyrus::vm::state::{Reader, Writer};
    let mut game = game(2.0, true, false);
    game.tick(0.5);
    let old_hash = game.world().state_hash();
    game.world_mut().actor(id(ACTOR)).unwrap().ai.wait_until = Some(9.0);
    assert_ne!(old_hash, game.world().state_hash());
    let original = game.world().save_state();
    let mut old_version = original.clone();
    old_version[4..6].copy_from_slice(&16u16.to_le_bytes());
    assert!(game.world_mut().load_state(&old_version).is_err());
    assert_eq!(game.world().save_state(), original);
    for deadline in [f64::NAN, f64::INFINITY, -1.0] {
        let mut bad = state(&game);
        bad.wait_until = Some(deadline);
        let mut writer = Writer::new();
        soe_game::ai::write_state(&mut writer, &bad);
        assert!(soe_game::ai::read_state(&mut Reader::new(&writer.out)).is_err());
    }
}

#[test]
fn worlds_saved_before_wait_tags_remain_readable() {
    let game = game(2.0, false, false);
    let original = game.world().save_state();
    let mut legacy = legacy::without_exclusive_branches(&mut game.world_mut(), &[id(ACTOR)]);
    // Version 21 predates the image-space, loose-item and zone blocks, and
    // also predates detection (read gate version 30) and save sections
    // (version 37), so the extra 6 bytes those two now always write at the
    // tail, regardless of version, must be truncated away too.
    legacy.truncate(legacy.len() - 35);
    legacy[4..6].copy_from_slice(&21u16.to_le_bytes());
    game.world_mut().load_state(&legacy).unwrap();
    assert_eq!(game.world().save_state(), original);
}

#[test]
fn installed_wait_branches_have_seconds_then_movement_inputs() {
    let Ok(_data) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let order = installed::load_order(&["Skyrim.esm"]).unwrap();
    let mut count = 0;
    for (id, _) in order.winners_of(T(b"PACK")) {
        let record = order.record(order.winner_ref(id).unwrap());
        let p = package::Package::from_record(record).unwrap();
        for branch in &p.tree {
            if branch.procedure.as_deref() != Some("Wait") {
                continue;
            }
            assert_eq!(branch.inputs.len(), 2, "{id:?}");
            assert!(
                matches!(
                    p.input(branch.inputs[0] as i8).map(|i| i.value),
                    Some(package::DataValue::Float(_))
                ),
                "{id:?}"
            );
            assert!(
                matches!(
                    p.input(branch.inputs[1] as i8).map(|i| i.value),
                    Some(package::DataValue::Bool(_))
                ),
                "{id:?}"
            );
            count += 1;
        }
    }
    assert!(count > 0);
    eprintln!("{count} vanilla Wait branches have float duration then Boolean movement inputs");
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based edge checks for SCR-14 Actor bindings.

#[path = "scr_13_actor_native_batch.rs"]
mod batch;

use soe_game::world::ActorKeepOffset;
use soe_papyrus::Value;

fn actor_id(game: &soe_game::Game, name: &str) -> soe_esm::GlobalFormId {
    game.form(name).unwrap()
}

fn materialize_actor(game: &soe_game::Game, actor: soe_esm::GlobalFormId) {
    let _ = game.world_mut().reference(actor);
}

#[test]
fn breaker_adversarial_case_required_offset_coordinates_are_not_silently_defaulted() {
    let mut game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    let criminal = actor_id(&game, "CriminalRef");
    materialize_actor(&game, guard);
    materialize_actor(&game, criminal);
    let target = batch::object(&mut game, criminal);
    let guard_object = batch::object(&mut game, guard);
    let original = ActorKeepOffset {
        target: criminal,
        offset: [8.0, 9.0, 10.0],
        angles: [1.0, 2.0, 3.0],
        catch_up_radius: 25.0,
        follow_radius: 4.0,
    };
    assert!(game.world_mut().keep_actor_offset(guard, original));

    batch::call(&mut game, guard_object, "KeepOffsetFromActor", &[target]);

    assert_eq!(game.world_mut().actor(guard).unwrap().keep_offset, Some(original));
}

#[test]
fn breaker_adversarial_case_zero_and_equal_radii_are_valid_ck_boundary_values() {
    let mut game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    materialize_actor(&game, guard);
    let guard_object = batch::object(&mut game, guard);

    batch::call(
        &mut game,
        guard_object.clone(),
        "KeepOffsetFromActor",
        &[
            guard_object,
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(-100.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(0.0),
        ],
    );

    assert_eq!(
        game.world_mut().actor(guard).unwrap().keep_offset,
        Some(ActorKeepOffset {
            target: guard,
            offset: [0.0, 0.0, -100.0],
            angles: [0.0; 3],
            catch_up_radius: 0.0,
            follow_radius: 0.0,
        })
    );
}

#[test]
fn breaker_adversarial_case_invalid_offset_target_does_not_replace_the_previous_request() {
    let mut game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    let criminal = actor_id(&game, "CriminalRef");
    materialize_actor(&game, guard);
    materialize_actor(&game, criminal);
    let guard_object = batch::object(&mut game, guard);
    let original = ActorKeepOffset {
        target: criminal,
        offset: [1.0, 2.0, 3.0],
        angles: [4.0, 5.0, 6.0],
        catch_up_radius: 20.0,
        follow_radius: 5.0,
    };
    assert!(game.world_mut().keep_actor_offset(guard, original));

    batch::call(&mut game, guard_object, "KeepOffsetFromActor", &[Value::Int(77)]);

    assert_eq!(game.world_mut().actor(guard).unwrap().keep_offset, Some(original));
    assert!(game.traces().iter().any(|trace| trace.contains("invalid target")));
}

#[test]
fn breaker_adversarial_case_clear_offset_is_idempotent_and_does_not_clear_another_actor() {
    let mut game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    let criminal = actor_id(&game, "CriminalRef");
    materialize_actor(&game, guard);
    materialize_actor(&game, criminal);
    let guard_object = batch::object(&mut game, guard);
    let criminal_object = batch::object(&mut game, criminal);
    let request = ActorKeepOffset {
        target: guard,
        offset: [0.0; 3],
        angles: [0.0; 3],
        catch_up_radius: 20.0,
        follow_radius: 5.0,
    };
    assert!(game.world_mut().keep_actor_offset(guard, request));
    assert!(game.world_mut().keep_actor_offset(criminal, request));

    batch::call(&mut game, guard_object.clone(), "ClearKeepOffsetFromActor", &[]);
    batch::call(&mut game, guard_object, "ClearKeepOffsetFromActor", &[]);

    assert_eq!(game.world_mut().actor(guard).unwrap().keep_offset, None);
    assert_eq!(game.world_mut().actor(criminal).unwrap().keep_offset, Some(request));
    drop(criminal_object);
}

#[test]
fn breaker_adversarial_case_forced_landing_marker_none_clears_and_invalid_marker_preserves_state() {
    let mut game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    let criminal = actor_id(&game, "CriminalRef");
    materialize_actor(&game, guard);
    materialize_actor(&game, criminal);
    let guard_object = batch::object(&mut game, guard);
    let criminal_object = batch::object(&mut game, criminal);
    batch::call(
        &mut game,
        guard_object.clone(),
        "SetForcedLandingMarker",
        &[criminal_object],
    );
    assert_eq!(game.world_mut().actor(guard).unwrap().forced_landing_marker, Some(criminal));

    batch::call(
        &mut game,
        guard_object.clone(),
        "SetForcedLandingMarker",
        &[Value::Int(77)],
    );
    assert_eq!(game.world_mut().actor(guard).unwrap().forced_landing_marker, Some(criminal));

    batch::call(&mut game, guard_object, "SetForcedLandingMarker", &[Value::None]);

    assert_eq!(game.world_mut().actor(guard).unwrap().forced_landing_marker, None);
}

#[test]
fn breaker_adversarial_case_offset_and_forced_marker_survive_world_save_load_together() {
    let game = batch::game();
    let guard = actor_id(&game, "GuardRef");
    let criminal = actor_id(&game, "CriminalRef");
    materialize_actor(&game, guard);
    materialize_actor(&game, criminal);
    let request = ActorKeepOffset {
        target: criminal,
        offset: [-3.0, 8.0, 12.0],
        angles: [90.0, -45.0, 180.0],
        catch_up_radius: 40.0,
        follow_radius: 11.0,
    };
    assert!(game.world_mut().keep_actor_offset(guard, request));
    assert!(game
        .world_mut()
        .set_forced_landing_marker(guard, Some(criminal)));
    let state = game.world_mut().save_state();
    game.world_mut().actor(guard).unwrap().keep_offset = None;
    game.world_mut().actor(guard).unwrap().forced_landing_marker = None;

    game.world_mut().load_state(&state).unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(guard).unwrap();
    assert_eq!(actor.keep_offset, Some(request));
    assert_eq!(actor.forced_landing_marker, Some(criminal));
}

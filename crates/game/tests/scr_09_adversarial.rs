// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "scr_09_actor_queries.rs"]
mod actor_queries;

use actor_queries::{actor_bool, fixture, id, Motion, Observation};

#[test]
fn scr_09_detection_query_is_observer_specific_and_missing_actor_is_an_error() {
    let game = fixture();
    let observation = Observation {
        observer: id(0x901),
        target: id(0x900),
        distance: 0.0,
        line_of_sight: true,
        light_level: 100.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    };
    game.world_mut()
        .evaluate_detection(&[observation], 1.0)
        .unwrap();
    assert!(game
        .world()
        .is_actor_detected_by(id(0x900), id(0x901))
        .unwrap());
    assert!(!game
        .world()
        .is_actor_detected_by(id(0x900), id(0x902))
        .unwrap());
    assert!(game
        .world()
        .is_actor_detected_by(id(0x903), id(0x901))
        .is_err());
}

#[test]
fn scr_09_actor_state_queries_read_shared_mount_crime_and_command_state() {
    let mut game = fixture();
    assert!(!actor_bool(&mut game, id(0x900), "IsCommandedActor", &[]));
    assert!(!actor_bool(&mut game, id(0x900), "IsOnMount", &[]));
    assert!(!actor_bool(&mut game, id(0x900), "IsTrespassing", &[]));

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.riding_mount = true;
        actor.trespassing = true;
    }
    assert!(actor_bool(&mut game, id(0x900), "IsOnMount", &[]));
    assert!(actor_bool(&mut game, id(0x900), "IsTrespassing", &[]));
}

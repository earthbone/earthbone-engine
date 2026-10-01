// SPDX-License-Identifier: GPL-3.0-or-later

include!("scr_09_actor_queries.rs");

#[test]
fn scr_09_detection_query_is_observer_specific_and_missing_actor_is_an_error() {
    let mut game = fixture();
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
        .is_actor_detected_by(id(0x902), id(0x901))
        .is_err());
}

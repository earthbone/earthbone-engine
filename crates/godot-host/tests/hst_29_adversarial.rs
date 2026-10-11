// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Edge cases for host locomotion inputs derived from World placements.

use soe_game::world::RefPlacement;
use soe_godot_host::actor::{npc_transform, NpcMotion};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x803,
};

fn placement(position: [f32; 3], heading: f32) -> RefPlacement {
    RefPlacement {
        position,
        rotation: [0.0, 0.0, heading],
        scale: 1.0,
        visible: true,
    }
}

fn patrol_game() -> Game {
    let template = package::close_header(package::build_template(0x800, "WalkingPatrol"), None)
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(
            T(b"PTDA"),
            [
                0i32.to_le_bytes(),
                0x804u32.to_le_bytes(),
                0i32.to_le_bytes(),
            ]
            .concat(),
        )
        .sub(T(b"UNAM"), [7]);
    let template = package::with_float_input(template, 11, 0.0);
    let template = package::with_bool_input(template, 3, false);
    let template = package::with_bool_input(template, 9, false);
    let template = package::with_bool_input(template, 14, false);
    let template = package::with_bool_input(template, 15, false);
    let template = package::with_procedure(
        package::finish(template),
        "Patrol",
        &[7, 11, 3, 9, 14, 15],
        false,
    );
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template);
    plugin.push_top(package::finish(package::close_header(
        package::build_record(0x801, "Patrol", 0, Schedule::default()),
        Some(0x800),
    )));
    plugin.push_top(
        npc::build_record(0x802, "Walker", "Walker", Config::default(), Stats::default())
            .sub(T(b"PKID"), 0x801u32.to_le_bytes()),
    );
    let mut actor_data = vec![0; 24];
    actor_data[20..24].copy_from_slice(&std::f32::consts::FRAC_PI_2.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR.object)
            .sub(T(b"NAME"), 0x802u32.to_le_bytes())
            .sub(T(b"DATA"), actor_data),
    );
    for (form, x, next) in [(0x804, 110.0f32, Some(0x805u32)), (0x805, 330.0, None)] {
        let mut data = vec![0; 24];
        data[..4].copy_from_slice(&x.to_le_bytes());
        let mut marker = Record::new(T(b"REFR"), form)
            .sub(T(b"NAME"), 0u32.to_le_bytes())
            .sub(T(b"DATA"), data)
            .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
            .sub(T(b"XPPA"), [])
            .sub(T(b"INAM"), 0u32.to_le_bytes());
        if let Some(next) = next {
            marker = marker.sub(T(b"XLKR"), next.to_le_bytes());
        }
        plugin.push_top(marker);
    }
    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(ACTOR));
    game
}

#[test]
fn hst_29_cornering_uses_the_destination_heading() {
    let before = placement([0.0, 0.0, 0.0], 0.0);
    let after = placement([10.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2);
    let motion = NpcMotion::between(before, after, 0.5).unwrap();

    assert!((motion.speed - 20.0).abs() < 0.001);
    assert!(motion.direction.abs() < 0.001);
    assert!((motion.turn - 180.0).abs() < 0.001);
}

#[test]
fn hst_29_patrol_host_transform_tracks_game_each_frame() {
    let mut game = patrol_game();
    let mut previous = game.world().ref_placement(ACTOR).unwrap();
    let mut moving_frames = 0;
    for _ in 0..300 {
        game.tick(1.0 / 60.0);
        let current = game.world().ref_placement(ACTOR).unwrap();
        let host = npc_transform(current);
        let position = host.origin.to_skyrim();
        let error = [position.x, position.y, position.z]
            .into_iter()
            .zip(current.position)
            .map(|(host, game)| (host - game).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!(error <= 8.0, "host transform trails Game by {error} units");
        let motion = NpcMotion::between(previous, current, 1.0 / 60.0).unwrap();
        if motion.speed > 0.1 {
            moving_frames += 1;
            assert!(motion.direction.abs() < 0.01);
        }
        previous = current;
    }
    assert!(moving_frames > 60, "Patrol must move during the observed frames");
}

#[test]
fn hst_29_reverse_motion_is_opposite_the_actor_heading() {
    let before = placement([0.0; 3], 0.0);
    let after = placement([0.0, -25.0, 0.0], 0.0);
    let motion = NpcMotion::between(before, after, 0.5).unwrap();

    assert!((motion.speed - 50.0).abs() < 0.001);
    assert!((motion.direction.abs() - 180.0).abs() < 0.001);
    assert_eq!(motion.turn, 0.0);
}

#[test]
fn hst_29_vertical_displacement_contributes_to_world_speed() {
    let before = placement([0.0; 3], 0.0);
    let after = placement([0.0, 0.0, 12.0], 0.0);
    let motion = NpcMotion::between(before, after, 0.25).unwrap();

    assert!((motion.speed - 48.0).abs() < 0.001);
    assert_eq!(motion.direction, 0.0);
}

#[test]
fn hst_29_transform_preserves_world_position_and_heading() {
    let source = placement([125.0, -40.0, 8.0], std::f32::consts::FRAC_PI_2);
    let transform = npc_transform(source);
    let position = transform.origin.to_skyrim();

    assert!((position.x - source.position[0]).abs() < 0.001);
    assert!((position.y - source.position[1]).abs() < 0.001);
    assert!((position.z - source.position[2]).abs() < 0.001);
    let forward = [transform.basis[0][2], transform.basis[1][2], transform.basis[2][2]];
    assert!(forward[0] < -0.99);
}

#[test]
fn hst_29_motion_rejects_nonfinite_placement_coordinates() {
    let before = placement([0.0; 3], 0.0);
    let after = placement([f32::NAN, 0.0, 0.0], 0.0);

    assert!(NpcMotion::between(before, after, 1.0).is_none());
}

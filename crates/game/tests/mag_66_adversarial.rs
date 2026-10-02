// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x101;
const BASE: u32 = 0x500;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("GrabActorPose.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn mag_66_held_and_released_actor_pose_survives_engine_save_and_load() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        assert!(world.set_host_actor_pose(id(ACTOR), [12.0, -34.0, 56.0], [0.1, 0.2, 0.3]));
    }

    let saved = game.save_state().unwrap();
    let mut loaded = fixture();
    loaded.load_state(&saved).unwrap();

    let placement = loaded.world().ref_placement(id(ACTOR)).unwrap();
    assert_eq!(placement.position, [12.0, -34.0, 56.0]);
    assert_eq!(placement.rotation, [0.1, 0.2, 0.3]);
}

#[test]
fn mag_66_host_pose_rejects_missing_disabled_or_deleted_actors() {
    let game = fixture();
    let mut world = game.world_mut();
    assert!(!world.set_host_actor_pose(id(0x999), [1.0; 3], [0.0; 3]));

    world.reference(id(ACTOR)).unwrap().enabled = false;
    assert!(!world.set_host_actor_pose(id(ACTOR), [1.0; 3], [0.0; 3]));
    let actor = world.reference(id(ACTOR)).unwrap();
    actor.enabled = true;
    actor.deleted = true;
    assert!(!world.set_host_actor_pose(id(ACTOR), [2.0; 3], [0.0; 3]));
}

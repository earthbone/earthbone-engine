// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! CMB-27 edge checks for the actor graph stagger lifecycle.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::godot_actor::{StaggerGraphEvent, StaggerTracker};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_actor() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("StaggerEdges.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn cmb_27_expired_deadline_seen_first_does_not_start_a_reaction() {
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 4.0;
        let actor = world.actor(id(0x900)).unwrap();
        actor.stagger_end_time = Some(3.0);
        actor.stagger_magnitude = 7.0;
    }

    assert_eq!(tracker.poll(&mut shared.borrow_mut(), id(0x900)), None);
}

#[test]
fn cmb_27_magnitude_change_restarts_active_reaction_even_without_deadline_change() {
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 1.0;
        let actor = world.actor(id(0x900)).unwrap();
        actor.stagger_end_time = Some(4.0);
        actor.stagger_magnitude = 2.0;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), id(0x900)),
        Some((StaggerGraphEvent::Start, 2.0))
    );

    shared
        .borrow_mut()
        .actor(id(0x900))
        .unwrap()
        .stagger_magnitude = 5.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), id(0x900)),
        Some((StaggerGraphEvent::Start, 5.0))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), id(0x900)), None);
}

#[test]
fn cmb_27_expiry_is_inclusive_and_a_later_hit_can_start_again() {
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 1.0;
        let actor = world.actor(id(0x900)).unwrap();
        actor.stagger_end_time = Some(2.0);
        actor.stagger_magnitude = 1.0;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), id(0x900)),
        Some((StaggerGraphEvent::Start, 1.0))
    );
    shared.borrow_mut().clock.real_seconds = 2.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), id(0x900)),
        Some((StaggerGraphEvent::Stop, 1.0))
    );

    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 2.1;
        let actor = world.actor(id(0x900)).unwrap();
        actor.stagger_end_time = Some(3.1);
        actor.stagger_magnitude = 3.0;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), id(0x900)),
        Some((StaggerGraphEvent::Start, 3.0))
    );
}

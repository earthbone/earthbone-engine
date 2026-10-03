// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! CMB-27 regression checks for stagger graph transitions.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::godot_actor::{StaggerGraphEvent, StaggerTracker};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_actor() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn cmb_27_clearing_an_active_deadline_stops_the_reaction_once() {
    let target = id(0x900);
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();

    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 2.0;
        let actor = world.actor(target).unwrap();
        actor.stagger_end_time = Some(3.0);
        actor.stagger_magnitude = 1.0;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 1.0))
    );

    shared.borrow_mut().actor(target).unwrap().stagger_end_time = None;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 1.0))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
}

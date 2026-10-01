// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-58 adversarial checks for event waits and graph unloads.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::vm::LatentId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), BASE));
    let mut order = LoadOrder::new();
    order.add("Anm58.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn anm_58_matching_event_completes_every_waiter_once_and_keeps_other_sources() {
    let mut game = game();
    let player = game.player();
    let (first, second) = {
        let mut world = game.world_mut();
        (
            world.place_at(id(BASE), player, false),
            world.place_at(id(BASE), player, false),
        )
    };
    {
        let mut world = game.world_mut();
        world.wait_for_animation_event(first, "done", LatentId(1));
        world.wait_for_animation_event(first, "done", LatentId(2));
        world.wait_for_animation_event(second, "done", LatentId(3));
    }
    game.world_mut()
        .animation_graph_event(first, "done", &mut |_, _| panic!("no listeners registered"));
    assert_eq!(
        game.world().completed_animation_waits,
        [(LatentId(1), true), (LatentId(2), true)]
    );
    assert_eq!(game.world().animation_waiters.len(), 1);
    game.world_mut()
        .animation_graph_event(first, "done", &mut |_, _| panic!("no listeners registered"));
    assert_eq!(game.world().completed_animation_waits.len(), 2);
}

#[test]
fn anm_58_graph_unload_only_completes_waits_for_unloaded_reference() {
    let mut game = game();
    let player = game.player();
    let (source, other) = {
        let mut world = game.world_mut();
        (
            world.place_at(id(BASE), player, false),
            world.place_at(id(BASE), player, false),
        )
    };
    game.world_mut()
        .wait_for_animation_event(source, "finished", LatentId(4));
    game.world_mut()
        .wait_for_animation_event(other, "finished", LatentId(5));
    game.world_mut().animation_graph_unloaded(source);
    assert_eq!(
        game.world().completed_animation_waits,
        [(LatentId(4), false)]
    );
    assert_eq!(game.world().animation_waiters.len(), 1);
}

#[test]
fn anm_58_nonmatching_event_does_not_resolve_wait_or_duplicate_event_delivery() {
    let mut game = game();
    let player = game.player();
    let source = game.world_mut().place_at(id(BASE), player, false);
    game.world_mut()
        .wait_for_animation_event(source, "done", LatentId(6));
    game.world_mut()
        .animation_graph_event(source, "other", &mut |_, _| {
            panic!("no listeners registered")
        });
    assert!(game.world().completed_animation_waits.is_empty());
    assert_eq!(game.world().animation_waiters.len(), 1);
    assert_eq!(game.world().animation_events, [(source, "other".into())]);
}

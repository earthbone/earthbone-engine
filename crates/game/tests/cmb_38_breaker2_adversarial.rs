// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Check area knock boundaries and disabled actor handling.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn cmb_38_area_knock_ignores_disabled_and_outside_actors() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), 0x100));
    plugin.push_top(Record::new(T(b"NPC_"), 0x101));
    let mut order = LoadOrder::new();
    order.add("Cmb38Boundary.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let (source, inside_disabled, outside) = {
        let mut world = game.world_mut();
        let source = world.place_at(id(0x100), player, false);
        let inside_disabled = world.place_at(id(0x101), player, false);
        let outside = world.place_at(id(0x101), player, false);
        world.reference(inside_disabled).unwrap().position = [2.0, 0.0, 0.0];
        world.reference(inside_disabled).unwrap().enabled = false;
        world.reference(outside).unwrap().position = [10.01, 0.0, 0.0];
        (source, inside_disabled, outside)
    };

    game.world_mut()
        .knock_area_effect(source, 1.0, 10.0)
        .unwrap();

    assert!(game.world_mut().take_actor_impulses(inside_disabled).is_empty());
    assert!(game.world_mut().take_actor_impulses(outside).is_empty());
}

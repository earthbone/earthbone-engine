// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-38 checks for repeated impulses, invalid references and saved state.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const MARKER: u32 = 0x100;
const ACTOR: u32 = 0x101;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn make_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"ACTI"), MARKER));
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    let mut order = LoadOrder::new();
    order.add("Cmb38.esm", plugin).unwrap();
    Game::new(order)
}

fn place(game: &mut Game) -> (GlobalFormId, GlobalFormId) {
    let player = game.player();
    let mut world = game.world_mut();
    let marker = world.place_at(id(MARKER), player, false);
    let actor = world.place_at(id(ACTOR), player, false);
    (marker, actor)
}

#[test]
fn cmb_38_rejected_pushes_do_not_change_actor_or_leave_partial_impulses() {
    let mut game = make_game();
    let (marker, actor) = place(&mut game);
    let mut world = game.world_mut();
    assert!(world.push_actor_away(id(0x999), actor, 10.0).is_err());
    assert!(world.push_actor_away(marker, marker, 10.0).is_err());
    assert!(world.push_actor_away(marker, actor, f32::INFINITY).is_err());
    assert!(world.take_actor_impulses(actor).is_empty());
    assert!(!world.actor(actor).unwrap().unconscious);
}

#[test]
fn cmb_38_repeated_pushes_preserve_each_signed_force_and_are_consumed_once() {
    let mut game = make_game();
    let (marker, actor) = place(&mut game);
    game.world_mut().reference(actor).unwrap().position = [3.0, 4.0, 0.0];
    game.world_mut().push_actor_away(marker, actor, 5.0).unwrap();
    game.world_mut().push_actor_away(marker, actor, -2.0).unwrap();

    let mut world = game.world_mut();
    let impulses = world.take_actor_impulses(actor);
    assert_eq!(impulses.len(), 2);
    assert_eq!(impulses[0].vector, [3.0, 4.0, 0.0]);
    assert_eq!(impulses[1].vector, [-1.2, -1.6, 0.0]);
    assert!(world.take_actor_impulses(actor).is_empty());
    assert!(world.actor(actor).unwrap().unconscious);
}

#[test]
fn cmb_38_knockdown_and_ragdoll_override_survive_world_save_load() {
    let mut game = make_game();
    let (marker, actor) = place(&mut game);
    game.world_mut().force_ragdoll(actor, true).unwrap();
    let saved = game.world_mut().save_state();

    let mut loaded = make_game();
    loaded.world_mut().load_state(&saved).unwrap();
    let mut world = loaded.world_mut();
    assert_eq!(world.actor(actor).unwrap().ragdoll_forced, Some(true));

    world.force_ragdoll(actor, false).unwrap();
    world.push_actor_away(marker, actor, 10.0).unwrap();
    let saved = world.save_state();
    drop(world);
    let mut reloaded = make_game();
    reloaded.world_mut().load_state(&saved).unwrap();
    let mut world = reloaded.world_mut();
    assert_eq!(world.actor(actor).unwrap().ragdoll_forced, Some(false));
    assert!(world.actor(actor).unwrap().unconscious);
    assert!(world.take_actor_impulses(actor).is_empty());
}

#[test]
fn cmb_38_area_zero_magnitude_does_not_knock_down_or_leave_impulse() {
    let mut game = make_game();
    let (marker, actor) = place(&mut game);
    game.world_mut().reference(actor).unwrap().position = [1.0, 0.0, 0.0];
    game.world_mut().knock_area_effect(marker, 0.0, 2.0).unwrap();
    let mut world = game.world_mut();
    assert!(world.take_actor_impulses(actor).is_empty());
    assert!(!world.actor(actor).unwrap().unconscious);
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-38 adversarial checks for CK push, knock and ragdoll contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const MARKER: u32 = 0x100;
const ACTOR: u32 = 0x101;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), MARKER));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR));
    let mut order = LoadOrder::new();
    order.add("Cmb38.esm", plugin).unwrap();
    Game::new(order)
}

fn place(game: &mut Game) -> (GlobalFormId, GlobalFormId) {
    let player = game.player();
    let (marker, actor) = {
        let mut world = game.world_mut();
        let marker = world.place_at(id(MARKER), player, false);
        let actor = world.place_at(id(ACTOR), player, false);
        (marker, actor)
    };
    (marker, actor)
}

#[test]
fn cmb_38_push_negative_force_pulls_toward_source() {
    let mut game = game();
    let (marker, actor) = place(&mut game);
    game.world_mut().reference(actor).unwrap().position = [3.0, 4.0, 0.0];
    game.world_mut()
        .push_actor_away(marker, actor, -10.0)
        .unwrap();
    assert_eq!(
        game.world_mut().take_actor_impulses(actor)[0].vector,
        [-6.0, -8.0, 0.0]
    );
}

#[test]
fn cmb_38_zero_magnitude_and_actor_at_radius_are_no_motion() {
    let mut game = game();
    let (marker, actor) = place(&mut game);
    game.world_mut().reference(actor).unwrap().position = [10.0, 0.0, 0.0];
    game.world_mut()
        .knock_area_effect(marker, 0.0, 10.0)
        .unwrap();
    let impulses = game.world_mut().take_actor_impulses(actor);
    assert!(impulses.is_empty(), "zero magnitude must not queue a knock");
    assert!(!game.world_mut().actor(actor).unwrap().unconscious);
}

#[test]
fn cmb_38_force_remove_does_not_clear_an_independent_unconscious_state() {
    let mut game = game();
    let (_, actor) = place(&mut game);
    game.world_mut().actor(actor).unwrap().unconscious = true;
    game.world_mut().force_ragdoll(actor, true).unwrap();
    game.world_mut().force_ragdoll(actor, false).unwrap();
    let mut world = game.world_mut();
    let state = world.actor(actor).unwrap();
    assert_eq!(state.ragdoll_forced, Some(false));
    assert!(
        state.unconscious,
        "removing ragdoll must not wake the actor"
    );
}

#[test]
fn cmb_38_area_magnitude_and_radius_follow_the_ck_argument_contract() {
    let mut game = game();
    let (marker, actor) = place(&mut game);
    game.world_mut().reference(actor).unwrap().position = [5.0, 0.0, 0.0];

    for (magnitude, radius) in [(-0.01, 10.0), (1.01, 10.0), (0.5, 0.0), (0.5, f32::NAN)] {
        assert!(game
            .world_mut()
            .knock_area_effect(marker, magnitude, radius)
            .is_err());
    }
    assert!(game.world_mut().take_actor_impulses(actor).is_empty());

    game.world_mut()
        .knock_area_effect(marker, 1.0, 10.0)
        .unwrap();
    let impulse = game.world_mut().take_actor_impulses(actor).remove(0);
    assert_eq!(impulse.vector, [0.5, 0.0, 0.0]);
    assert!(game.world_mut().actor(actor).unwrap().unconscious);
}

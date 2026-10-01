// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial HST-09 furniture activation checks.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CHAIR: u32 = 0x200;
const CHAIR_BASE: u32 = 0x300;
const ACTOR_BASE: u32 = 0x350;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_furniture_flags(flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), ACTOR_BASE));
    plugin.push_top(
        Record::new(T(b"FURN"), CHAIR_BASE)
            .sub(T(b"MNAM"), flags.to_le_bytes())
            .sub(T(b"FNPR"), [1_u8, 0, 0, 0]),
    );
    plugin.push_top(Record::new(T(b"REFR"), CHAIR).sub(T(b"NAME"), CHAIR_BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"REFR"), 0x201).sub(T(b"NAME"), ACTOR_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;
    game
}

#[test]
fn hst_09_mnam_disables_activation_even_when_a_sit_marker_is_active() {
    // TES5Edit identifies MNAM 0x02000000 as "Disables Activation".
    let mut game = game_with_furniture_flags(0x0200_0001);
    let player = game.player();

    game.activate(id(CHAIR), player);

    assert_eq!(
        game.world().furniture_seat(player),
        None,
        "a furniture record with activation disabled cannot seat or reserve the actor"
    );
}

#[test]
fn hst_09_reserved_marker_cannot_be_taken_by_a_second_actor() {
    let mut game = game_with_furniture_flags(1);
    let first = game.player();
    let second = id(0x201);
    game.world_mut().reference(second).unwrap().is_actor = true;

    assert!(game.activate(id(CHAIR), first));
    game.activate(id(CHAIR), second);

    assert_eq!(game.world().furniture_seat(first), Some((id(CHAIR), 0)));
    assert_eq!(game.world().furniture_seat(second), None);
    assert!(game.world().furniture_marker_in_use(id(CHAIR), Some(0)));
}

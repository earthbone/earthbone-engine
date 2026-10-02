// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-09 chair marker reservation, seating and standing behavior.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CHAIR: u32 = 0x200;
const CHAIR_BASE: u32 = 0x300;
const ACTOR_BASE: u32 = 0x350;
const ACTOR_TWO: u32 = 0x400;
const ACTOR_THREE: u32 = 0x401;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order() -> LoadOrder {
    order_with_markers(3, [1, 1])
}

fn order_with_markers(active_markers: u32, marker_types: [u16; 2]) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), ACTOR_BASE));
    let mut furniture =
        Record::new(T(b"FURN"), CHAIR_BASE).sub(T(b"MNAM"), active_markers.to_le_bytes());
    for marker_type in marker_types {
        let marker_type = marker_type.to_le_bytes();
        furniture = furniture.sub(T(b"FNPR"), [marker_type[0], marker_type[1], 0, 0]);
    }
    plugin.push_top(furniture);
    plugin.push_top(
        Record::new(T(b"REFR"), CHAIR)
            .sub(T(b"NAME"), CHAIR_BASE.to_le_bytes())
            .sub(
                T(b"DATA"),
                Transform {
                    position: [25.0, 50.0, 75.0],
                    rotation: [0.0, 0.0, 1.5],
                }
                .to_bytes(),
            ),
    );
    for actor in [ACTOR_TWO, ACTOR_THREE] {
        plugin.push_top(Record::new(T(b"REFR"), actor).sub(T(b"NAME"), ACTOR_BASE.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

#[test]
fn hst_09_chair_activation_reserves_marker_seats_actor_and_standing_releases_it() {
    let mut game = Game::new(order());
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;
    for actor in [id(ACTOR_TWO), id(ACTOR_THREE)] {
        let mut world = game.world_mut();
        let reference = world.reference(actor).unwrap();
        reference.is_actor = true;
        reference.base = None;
    }

    assert!(game.activate(id(CHAIR), player));
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        [25.0, 50.0, 75.0],
        "seating places the actor at the furniture reference"
    );
    assert_eq!(
        game.world().furniture_seat(player),
        Some((id(CHAIR), 0)),
        "the first active Sit marker is reserved for the seated actor"
    );

    assert!(game.activate(id(CHAIR), id(ACTOR_TWO)));
    assert_eq!(
        game.world().furniture_seat(id(ACTOR_TWO)),
        Some((id(CHAIR), 1)),
        "a second actor reserves the next free marker"
    );
    assert!(game.world().furniture_marker_in_use(id(CHAIR), Some(0)));

    let saved = game.world().save_state();
    let mut restored = soe_game::World::new(order());
    restored.load_state(&saved).unwrap();
    assert!(restored.furniture_marker_in_use(id(CHAIR), Some(0)));
    assert!(restored.stand_from_furniture(player));
    assert!(!restored.furniture_marker_in_use(id(CHAIR), Some(0)));
    assert!(restored.activate_furniture(id(CHAIR), id(ACTOR_THREE)));
    assert_eq!(
        restored.furniture_seat(id(ACTOR_THREE)),
        Some((id(CHAIR), 0)),
        "standing releases the marker for the next actor"
    );
}

#[test]
fn hst_09_only_active_sit_markers_can_be_reserved() {
    let mut game = Game::new(order_with_markers(1, [2, 1]));
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;

    assert!(game.activate(id(CHAIR), player));
    assert_eq!(game.world().furniture_seat(player), None);
}

#[test]
fn hst_09_activating_the_seated_chair_again_stands_and_releases_it() {
    let mut game = Game::new(order());
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;

    assert!(game.activate(id(CHAIR), player));
    assert_eq!(game.world().furniture_seat(player), Some((id(CHAIR), 0)));
    assert!(game.activate(id(CHAIR), player));
    assert_eq!(game.world().furniture_seat(player), None);
    assert!(!game.world().furniture_marker_in_use(id(CHAIR), Some(0)));
}

#[test]
fn hst_09_rejects_non_furniture_disabled_references_and_non_actors() {
    let mut game = Game::new(order());
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;
    for actor in [id(ACTOR_TWO), id(ACTOR_THREE)] {
        game.world_mut().reference(actor).unwrap().is_actor = true;
    }

    assert_eq!(
        game.world_mut()
            .reserve_furniture_marker(id(CHAIR), id(CHAIR)),
        None
    );
    assert_eq!(
        game.world_mut()
            .reserve_furniture_marker(id(ACTOR_TWO), player),
        None
    );
    game.world_mut().reference(id(CHAIR)).unwrap().enabled = false;
    assert_eq!(
        game.world_mut().reserve_furniture_marker(id(CHAIR), player),
        None
    );
    assert_eq!(game.world().furniture_seat(player), None);
}

#[test]
fn hst_09_malformed_fnpr_and_markers_above_index_23_are_not_reserved() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut furniture = Record::new(T(b"FURN"), CHAIR_BASE)
        .sub(T(b"MNAM"), u32::MAX.to_le_bytes())
        .sub(T(b"FNPR"), [1_u8, 0, 0]);
    for _ in 1..24 {
        furniture = furniture.sub(T(b"FNPR"), [2_u8, 0, 0, 0]);
    }
    furniture = furniture.sub(T(b"FNPR"), [1_u8, 0, 0, 0]);
    plugin.push_top(Record::new(T(b"STAT"), ACTOR_BASE));
    plugin.push_top(furniture);
    plugin.push_top(Record::new(T(b"REFR"), CHAIR).sub(T(b"NAME"), CHAIR_BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"REFR"), ACTOR_TWO).sub(T(b"NAME"), ACTOR_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(ACTOR_TWO)).unwrap().is_actor = true;

    assert_eq!(
        game.world_mut()
            .reserve_furniture_marker(id(CHAIR), id(ACTOR_TWO)),
        None,
        "a malformed entry is skipped and marker index 24 is outside the active MNAM mask"
    );
}

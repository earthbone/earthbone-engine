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

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::objective_map;

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn location_objective() -> Record {
    let mut target = 2_i32.to_le_bytes().to_vec();
    target.extend_from_slice(&[0, 0, 0, 0]);
    Record::new(tag(*b"QUST"), 0x100)
        .sub(tag(*b"DNAM"), [0; 12])
        .sub(tag(*b"QOBJ"), 10_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "Reach the location")
        .sub(tag(*b"QSTA"), target)
        .sub(tag(*b"ALLS"), 2_u32.to_le_bytes())
        .zsub(tag(*b"ALID"), "LocationTarget")
        .sub(tag(*b"ALFL"), 0x200_u32.to_le_bytes())
        .sub(tag(*b"ALED"), [])
}

fn bad_marker_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(location_objective());
    plugin.push_top(Record::new(tag(*b"DOOR"), 0x400));
    let mut transform = Vec::new();
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        transform.extend_from_slice(&value.to_le_bytes());
    }
    plugin.push_top(
        Record::new(tag(*b"REFR"), 0x301)
            .sub(tag(*b"NAME"), 0x400_u32.to_le_bytes())
            .sub(tag(*b"DATA"), transform),
    );
    plugin.push_top(Record::new(tag(*b"LCTN"), 0x200).sub(tag(*b"MNAM"), 0x301_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ui10Breaker.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.start_quest(id(0x100)));
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x100)).unwrap().active = true;
        world.objective(id(0x100), 10).unwrap().displayed = true;
    }
    game
}

#[test]
fn ui_10_location_marker_requires_stat_backing_record() {
    let mut game = bad_marker_game();

    let markers = objective_map::markers(&mut game);

    assert!(
        markers.is_empty(),
        "invalid MNAM target produced markers: {markers:?}"
    );
}

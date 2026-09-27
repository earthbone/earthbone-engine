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

#[test]
fn ui_10_deleted_target_is_not_visible_and_missing_alias_is_omitted() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    let mut target = 1_i32.to_le_bytes().to_vec();
    target.extend_from_slice(&[0, 0, 0, 0]);
    plugin.push_top(
        Record::new(tag(*b"QUST"), 0x100)
            .sub(tag(*b"DNAM"), dnam)
            .sub(tag(*b"QOBJ"), 1_u16.to_le_bytes())
            .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
            .zsub(tag(*b"NNAM"), "Reach the marker")
            .sub(tag(*b"QSTA"), target)
            .sub(tag(*b"ALST"), 1_u32.to_le_bytes())
            .zsub(tag(*b"ALID"), "Target")
            .sub(tag(*b"ALFR"), 0x300_u32.to_le_bytes())
            .sub(tag(*b"ALED"), []),
    );
    plugin.push_top(Record::new(tag(*b"STAT"), 0x400));
    let transform = [0.0_f32; 6].into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>();
    plugin.push_top(Record::new(tag(*b"REFR"), 0x300).sub(tag(*b"NAME"), 0x400_u32.to_le_bytes()).sub(tag(*b"DATA"), transform));
    let mut order = LoadOrder::new();
    order.add("Map.esm", plugin).unwrap();
    let mut game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x100)).unwrap().active = true;
        world.objective(id(0x100), 1).unwrap().displayed = true;
        world.reference(id(0x300)).unwrap().deleted = true;
    }

    let markers = objective_map::markers(&mut game);
    assert_eq!(markers.len(), 1);
    assert!(!markers[0].visible);
    assert_eq!(markers[0].reference, id(0x300));
}

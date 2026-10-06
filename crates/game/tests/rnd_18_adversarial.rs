// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn rnd_18_lgtm_data_accepts_optional_directional_ambient_block() {
    let mut data = vec![0; 64];
    data[0..4].copy_from_slice(&[20, 30, 40, 0]);
    data[4..8].copy_from_slice(&[50, 60, 70, 0]);
    data[8..12].copy_from_slice(&[80, 90, 100, 0]);
    data[12..16].copy_from_slice(&100.0_f32.to_le_bytes());
    data[16..20].copy_from_slice(&1000.0_f32.to_le_bytes());
    data[32..36].copy_from_slice(&800.0_f32.to_le_bytes());
    data[36..40].copy_from_slice(&2.0_f32.to_le_bytes());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"LGTM"), 40)
            .sub(b"DATA", data)
            .sub(b"DALC", [120; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 31)
            .sub(b"DATA", [1])
            .sub(b"LTMP", 40_u32.to_le_bytes()),
    );

    let mut order = LoadOrder::new();
    order.add("Lighting.esm", plugin).expect("load fixture");
    let light = World::new(order)
        .interior_lighting(id(31))
        .expect("valid 64-byte LGTM/DATA body");

    assert_eq!(light.ambient, [20.0 / 255.0, 30.0 / 255.0, 40.0 / 255.0]);
    assert_eq!(light.ambient_directions, [[120.0 / 255.0; 3]; 6]);
    assert_eq!(light.fog, [100.0, 1000.0, 2.0, 1.0]);
    assert_eq!(light.fog_clip, 800.0);
}

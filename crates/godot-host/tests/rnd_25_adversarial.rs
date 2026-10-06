// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-25 WTHR cloud speed edge cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::sky::Climate, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn climate() -> Climate {
    Climate {
        timing: [6.0, 8.0, 18.0, 20.0],
        moons: [false; 2],
        phase_days: 0,
        sun_texture: String::new(),
        glare_texture: String::new(),
        night_model: String::new(),
        weathers: Vec::new(),
    }
}

fn weather(speed_x: Option<Vec<u8>>, speed_y: Option<Vec<u8>>, layers: u32) -> Record {
    let colors = vec![0_u8; 17 * 16];
    let mut record = Record::new(Tag(*b"WTHR"), 10)
        .sub(b"NAM0", colors)
        .sub(b"DATA", vec![0; 19])
        .sub(b"FNAM", vec![0; 32])
        .sub(b"LNAM", layers.to_le_bytes());
    if let Some(speed) = speed_x {
        record = record.sub(b"QNAM", speed);
    }
    if let Some(speed) = speed_y {
        record = record.sub(b"RNAM", speed);
    }
    record
}

fn sky(record: Record) -> soe_game::world::sky::SkyFrame {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Clouds.esm", plugin).expect("add weather fixture");
    let world = World::new(order);
    world
        .sample_sky(id(10), &climate(), 12.0, 0.0)
        .expect("sample weather fixture")
}

#[test]
fn rnd_25_disabled_layers_keep_their_wthr_speed_indices() {
    let mut x = vec![127; 32];
    x[1] = 0;
    x[10] = 254;
    let mut y = vec![127; 32];
    y[1] = 254;
    y[10] = 0;
    let record = weather(Some(x), Some(y), 11)
        .sub(b"00TX", b"sky/disabled.dds\0".to_vec())
        .sub(b"10TX", b"sky/layer1.dds\0".to_vec())
        .sub(b":0TX", b"sky/layer10.dds\0".to_vec())
        .sub(b"NAM1", 1_u32.to_le_bytes());

    let frame = sky(record);
    assert_eq!(frame.clouds.len(), 2);
    assert_eq!(frame.clouds[0].texture, "sky/layer1.dds");
    assert_eq!(frame.clouds[0].speed, [-0.1, 0.1]);
    assert_eq!(frame.clouds[1].texture, "sky/layer10.dds");
    assert_eq!(frame.clouds[1].speed, [0.1, -0.1]);
}

#[test]
fn rnd_25_absent_wthr_speed_axis_is_stationary() {
    let mut x = vec![127; 32];
    x[0] = 0;
    let record = weather(Some(x), None, 1).sub(b"00TX", b"sky/layer0.dds\0".to_vec());

    let frame = sky(record);
    assert_eq!(frame.clouds.len(), 1);
    assert_eq!(frame.clouds[0].speed, [-0.1, 0.0]);
}

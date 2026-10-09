// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedModifiers};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn modifier(object: u32, duration: f32, amount: f32) -> Record {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1_u32.to_le_bytes());
    header[4..8].copy_from_slice(&duration.to_le_bytes());
    let channels = [
        (*b"TNAM", 176, true, vec![amount, amount / 2.0, amount / 4.0, amount]),
        (*b"BNAM", 180, false, vec![amount * 8.0]),
        (*b"WNAM", 212, false, vec![amount]),
        (*b"XNAM", 216, false, vec![amount * 140.0]),
        (*b"YNAM", 220, false, vec![amount * 280.0]),
        (*b"\x02IAD", 8 + 2 * 8, false, vec![amount]),
        (*b"BIAD", 8 + 2 * 8 + 4, false, vec![amount / 2.0]),
        (*b"\x03IAD", 8 + 3 * 8, false, vec![amount]),
        (*b"CIAD", 8 + 3 * 8 + 4, false, vec![amount / 2.0]),
        (*b"\x11IAD", 8 + 17 * 8, false, vec![1.0 + amount]),
        (*b"QIAD", 8 + 17 * 8 + 4, false, vec![amount / 2.0]),
        (*b"\x12IAD", 8 + 18 * 8, false, vec![1.0 + amount]),
        (*b"RIAD", 8 + 18 * 8 + 4, false, vec![amount / 2.0]),
        (*b"\x13IAD", 8 + 19 * 8, false, vec![1.0 + amount]),
        (*b"SIAD", 8 + 19 * 8 + 4, false, vec![amount / 2.0]),
    ];
    let mut record = Record::new(Tag::new(b"IMAD"), object);
    for (tag, count_offset, color, values) in channels {
        header[count_offset..count_offset + 4].copy_from_slice(&2_u32.to_le_bytes());
        let mut data = Vec::new();
        for time in [0.0_f32, 1.0] {
            data.extend_from_slice(&time.to_le_bytes());
            for value in &values {
                data.extend_from_slice(&(value * time).to_le_bytes());
            }
        }
        assert_eq!(data.len(), if color { 40 } else { 16 });
        record = record.sub(Tag::new(&tag), data);
    }
    record.sub(Tag::new(b"DNAM"), header)
}

#[test]
fn mag_71_two_animated_records_keep_independent_time_and_stack_weights() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(modifier(1, 2.0, 0.8));
    plugin.push_top(modifier(2, 4.0, 0.4));
    let mut order = LoadOrder::new();
    order.add("Mag71Adversarial.esm", plugin).unwrap();
    let mut game = Game::new(order);

    game.world_mut().apply_image_space(id(1), 0.25).unwrap();
    game.world_mut().apply_image_space(id(2), 0.75).unwrap();
    game.tick(0.5);

    let mut stack = ImageSpaceStack::default();
    stack.sync(&game.world()).unwrap();
    let layers = stack.render_layers(&game.world()).unwrap();
    assert_eq!(layers.len(), 2);
    assert_eq!((layers[0].strength, layers[1].strength), (0.25, 0.75));
    assert!((layers[0].channels[0].values[0] - 0.2).abs() < 0.00001);
    assert!((layers[1].channels[0].values[0] - 0.05).abs() < 0.00001);

    let rendered = RenderedModifiers::from_layers(&layers);
    assert!((rendered.tint[0] - 0.0875).abs() < 0.00001);
    assert!((rendered.blur_radius - 0.7).abs() < 0.00001);
    assert!((rendered.depth_strength - 0.0875).abs() < 0.00001);
    assert!(
        (rendered.depth_distance - 0.175).abs() < 0.00001,
        "depth distance: {}",
        rendered.depth_distance
    );
    assert!((rendered.depth_range - 1.35).abs() < 0.00001);
    assert!((rendered.bloom_threshold - 0.13125).abs() < 0.00001);
    assert!((rendered.bloom_scale - 0.13125).abs() < 0.00001);
    assert!((rendered.saturation - 0.2875).abs() < 0.00001);
    assert!((rendered.brightness - 0.2875).abs() < 0.00001);
    assert!((rendered.contrast - 0.2875).abs() < 0.00001);
}

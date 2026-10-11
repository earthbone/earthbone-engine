// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedModifiers};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn modifier(tint: f32, blur: f32, depth: f32, threshold_mult: f32, threshold_add: f32) -> Record {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1_u32.to_le_bytes());
    header[4..8].copy_from_slice(&2.0_f32.to_le_bytes());
    for offset in [176, 180, 212, 216, 24, 28] {
        header[offset..offset + 4].copy_from_slice(&2_u32.to_le_bytes());
    }

    let mut record = Record::new(Tag::new(b"IMAD"), 1);
    let curves: [([u8; 4], &[f32]); 6] = [
        (*b"TNAM", &[tint, tint, tint, 0.8]),
        (*b"BNAM", &[blur]),
        (*b"WNAM", &[depth]),
        (*b"XNAM", &[1400.0]),
        (*b"\x02IAD", &[threshold_mult]),
        (*b"BIAD", &[threshold_add]),
    ];
    for (tag, values) in curves {
        let mut data = Vec::new();
        for time in [0.0_f32, 1.0] {
            data.extend_from_slice(&time.to_le_bytes());
            for value in values {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        record = record.sub(Tag::new(&tag), data);
    }
    record.sub(Tag::new(b"DNAM"), header)
}

#[test]
fn mag_71_rendered_stack_uses_winning_override_for_animated_imad_channels() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(modifier(0.1, 0.5, 0.2, 0.5, 0.1));
    let mut patch = Plugin::new(FileHeader::default());
    patch.push_top(modifier(0.8, 4.0, 0.8, 1.2, 0.2));

    let mut order = LoadOrder::new();
    order.add("Mag71Base.esm", base).unwrap();
    order.add("Mag71Patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    game.world_mut().apply_image_space(id(1), 0.25).unwrap();
    game.tick(2.0);

    let mut stack = ImageSpaceStack::default();
    stack.sync(&game.world()).unwrap();
    let layers = stack.render_layers(&game.world()).unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].channels.len(), 6);
    let rendered = RenderedModifiers::from_layers(&layers);

    assert!((rendered.tint[0] - 0.2).abs() < 0.00001);
    assert!((rendered.blur_radius - 1.0).abs() < 0.00001);
    assert!((rendered.depth_strength - 0.2).abs() < 0.00001);
    assert!((rendered.depth_distance - 2.5).abs() < 0.00001);
    assert!((rendered.bloom_threshold - 0.875).abs() < 0.00001);
}

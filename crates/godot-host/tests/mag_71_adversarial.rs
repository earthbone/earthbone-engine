// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceRenderLayer, ImageSpaceStack, RenderedModifiers};

fn record(object: u32, channels: &[([u8; 4], usize, bool, Vec<f32>)]) -> Record {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1u32.to_le_bytes());
    header[4..8].copy_from_slice(&2.0f32.to_le_bytes());
    let mut record = Record::new(Tag::new(b"IMAD"), object);
    for (tag, offset, color, values) in channels {
        header[*offset..*offset + 4].copy_from_slice(&2u32.to_le_bytes());
        let mut data = Vec::new();
        for time in [0.0f32, 1.0] {
            data.extend_from_slice(&time.to_le_bytes());
            for value in values {
                data.extend_from_slice(&(value * (time + 1.0)).to_le_bytes());
            }
        }
        assert_eq!(data.len(), if *color { 40 } else { 16 });
        record = record.sub(Tag::new(tag), data);
    }
    record.sub(Tag::new(b"DNAM"), header)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_71_active_stack_retains_color_blur_depth_and_hdr_samples_at_layer_weights() {
    let channels = [
        (*b"TNAM", 176, true, vec![0.1, 0.2, 0.3, 0.4]),
        (*b"BNAM", 180, false, vec![0.2]),
        (*b"WNAM", 212, false, vec![0.3]),
        (*b"\x13IAD", 8 + 19 * 8, false, vec![0.4]),
    ];
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record(1, &channels));
    plugin.push_top(record(2, &channels));
    let mut order = LoadOrder::new();
    order.add("Mag71.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().apply_image_space(id(1), 0.25).unwrap();
    game.world_mut().apply_image_space(id(2), 0.75).unwrap();
    game.tick(1.0);
    let mut stack = ImageSpaceStack::default();
    stack.sync(&game.world()).unwrap();
    let frame = stack.render_layers(&game.world()).unwrap();
    assert_eq!(frame.len(), 2);
    assert_eq!((frame[0].strength, frame[1].strength), (0.25, 0.75));
    for layer in &frame {
        assert_eq!(
            layer
                .channels
                .iter()
                .map(|sample| sample.channel)
                .collect::<Vec<_>>(),
            vec![*b"TNAM", *b"BNAM", *b"WNAM", *b"\x13IAD"]
        );
        assert!((layer.channels[0].values[0] - 0.15).abs() < 0.00001);
        assert!((layer.channels[1].values[0] - 0.3).abs() < 0.00001);
        assert!((layer.channels[2].values[0] - 0.45).abs() < 0.00001);
        assert!((layer.channels[3].values[0] - 0.6).abs() < 0.00001);
    }
}

#[test]
fn mag_71_duplicate_authored_channel_rejects_the_frame() {
    let mut header = vec![0; 244];
    header[180..184].copy_from_slice(&1u32.to_le_bytes());
    let data = [0.0f32, 1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let record = Record::new(Tag::new(b"IMAD"), 1)
        .sub(Tag::new(b"DNAM"), header)
        .sub(Tag::new(b"BNAM"), data.clone())
        .sub(Tag::new(b"BNAM"), data);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("BadMag71.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().apply_image_space(id(1), 1.0).unwrap();
    let mut stack = ImageSpaceStack::default();
    stack.sync(&game.world()).unwrap();
    assert!(stack.render_layers(&game.world()).is_err());
}

#[test]
fn mag_71_composes_authored_render_channels_with_active_stack_weights() {
    let layers = vec![
        ImageSpaceRenderLayer {
            modifier: id(1),
            strength: 0.25,
            channels: vec![
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"TNAM",
                    values: vec![1.0, 0.0, 0.0, 0.8],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"BNAM",
                    values: vec![4.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"WNAM",
                    values: vec![0.8],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"XNAM",
                    values: vec![500.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"YNAM",
                    values: vec![200.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"\x03IAD",
                    values: vec![2.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"\x02IAD",
                    values: vec![2.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"CIAD",
                    values: vec![2.0],
                },
                soe_game::world::image_space::ImageSpaceCurveSample {
                    channel: *b"BIAD",
                    values: vec![2.0],
                },
            ],
        },
        ImageSpaceRenderLayer {
            modifier: id(2),
            strength: 0.5,
            channels: vec![soe_game::world::image_space::ImageSpaceCurveSample {
                channel: *b"TNAM",
                values: vec![0.0, 0.0, 1.0, 0.4],
            }],
        },
    ];

    let rendered = RenderedModifiers::from_layers(&layers);

    assert_eq!(rendered.tint, [0.25, 0.0, 0.5, 0.4]);
    assert_eq!(rendered.blur_radius, 1.0);
    assert_eq!(rendered.depth_strength, 0.2);
    assert_eq!(
        rendered.depth_distance,
        125.0 / soe_game::world::UNITS_PER_METRE
    );
    assert_eq!(
        rendered.depth_range,
        1.0 + 50.0 / soe_game::world::UNITS_PER_METRE
    );
    assert!((rendered.bloom_threshold - 1.525).abs() < 0.00001);
    assert_eq!(rendered.bloom_scale, 1.0);
}

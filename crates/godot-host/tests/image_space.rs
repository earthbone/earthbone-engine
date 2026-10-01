// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::image_space::WeatherSelection;
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedBase};

#[test]
fn mag_72_render_values_follow_cell_source_and_clear_on_replacement() {
    let mut plugin = Plugin::new(FileHeader::default());
    let floats = |values: &[f32]| {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>()
    };
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 10)
            .sub(
                Tag::new(b"HNAM"),
                floats(&[0.0, 0.0, 0.0, 0.4, 0.0, 4.0, 1.0, 1.0, 0.5]),
            )
            .sub(Tag::new(b"CNAM"), floats(&[0.6, 1.2, 1.4]))
            .sub(Tag::new(b"TNAM"), floats(&[0.25, 0.8, 0.2, 0.1]))
            .sub(
                Tag::new(b"DNAM"),
                [floats(&[0.5, 400.0, 200.0]), vec![1, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 11)
            .sub(
                Tag::new(b"HNAM"),
                floats(&[0.0, 0.0, 0.0, 0.8, 0.0, 8.0, 1.0, 1.0, 1.0]),
            )
            .sub(Tag::new(b"CNAM"), floats(&[0.4, 1.6, 1.1]))
            .sub(Tag::new(b"TNAM"), floats(&[0.5, 0.1, 0.8, 0.2]))
            .sub(
                Tag::new(b"DNAM"),
                [floats(&[0.8, 600.0, 300.0]), vec![1, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag::new(b"WTHR"), 30).sub(
            Tag::new(b"IMSP"),
            [10_u32, 11, 10, 11]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    plugin
        .push_top(Record::new(Tag::new(b"CELL"), 20).sub(Tag::new(b"XCIM"), 10_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 21));
    let mut order = LoadOrder::new();
    order.add("Render.esm", plugin).unwrap();
    let game = Game::new(order);
    let mut host = ImageSpaceStack::default();
    game.world_mut().load_cell(GlobalFormId {
        file: 0,
        object: 20,
    });
    host.sync(&game.world()).unwrap();
    let rendered = RenderedBase::from(host.base().unwrap());
    assert_eq!(
        (rendered.exposure, rendered.white, rendered.bloom),
        (1.5, 4.0, 0.4)
    );
    assert_eq!(
        (rendered.saturation, rendered.brightness, rendered.contrast),
        (0.6, 1.2, 1.4)
    );
    assert_eq!(
        (
            rendered.depth_amount,
            rendered.depth_distance,
            rendered.depth_range
        ),
        (0.5, 400.0 / 70.0, 200.0 / 70.0)
    );
    assert_eq!(rendered.tint, [0.25, 0.8, 0.2, 0.1]);
    game.world_mut()
        .select_weather_image_space(Some(WeatherSelection {
            weather: GlobalFormId {
                file: 0,
                object: 30,
            },
            from: 1,
            to: 1,
            blend: 0.0,
        }))
        .unwrap();
    host.sync(&game.world()).unwrap();
    let weather = RenderedBase::from(host.base().unwrap());
    assert_eq!(
        (weather.exposure, weather.white, weather.bloom),
        (2.0, 8.0, 0.8)
    );
    assert_eq!(
        (weather.saturation, weather.brightness, weather.contrast),
        (0.4, 1.6, 1.1)
    );
    assert_eq!(
        (
            weather.depth_amount,
            weather.depth_distance,
            weather.depth_range
        ),
        (0.8, 600.0 / 70.0, 300.0 / 70.0)
    );
    game.world_mut().select_weather_image_space(None).unwrap();
    game.world_mut().load_cell(GlobalFormId {
        file: 0,
        object: 21,
    });
    host.sync(&game.world()).unwrap();
    assert!(host.base().is_none());
}

#[test]
fn mag_71_host_frame_keeps_curve_samples_with_their_active_weights() {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1u32.to_le_bytes());
    header[4..8].copy_from_slice(&2.0f32.to_le_bytes());
    header[180..184].copy_from_slice(&2u32.to_le_bytes());
    let mut curve = Vec::new();
    for (time, value) in [(0.0f32, 0.0f32), (1.0, 1.0)] {
        curve.extend_from_slice(&time.to_le_bytes());
        curve.extend_from_slice(&value.to_le_bytes());
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"IMAD"), 1)
            .sub(Tag::new(b"DNAM"), header)
            .sub(Tag::new(b"BNAM"), curve),
    );
    let mut order = LoadOrder::new();
    order.add("HostImageCurves.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let modifier = GlobalFormId { file: 0, object: 1 };
    game.world_mut().apply_image_space(modifier, 0.4).unwrap();
    game.tick(1.0);
    let mut host = ImageSpaceStack::default();
    host.sync(&game.world()).unwrap();
    let frame = host.render_layers(&game.world()).unwrap();
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].strength, 0.4);
    assert_eq!(frame[0].channels[0].channel, *b"BNAM");
    assert!((frame[0].channels[0].values[0] - 0.5).abs() < 0.000_001);
}

#[test]
fn image_space_host_snapshot_follows_game_fades_and_replacement() {
    let mut plugin = Plugin::new(FileHeader::default());
    for id in [1, 2] {
        plugin.push_top(Record::new(Tag::new(b"IMAD"), id).sub(Tag::new(b"DNAM"), vec![0; 244]));
    }
    let mut order = LoadOrder::new();
    order.add("HostImages.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let first = GlobalFormId { file: 0, object: 1 };
    let second = GlobalFormId { file: 0, object: 2 };
    let mut host = ImageSpaceStack::default();
    game.world_mut().apply_image_space(first, 2.0).unwrap();
    game.world_mut()
        .crossfade_image_space(Some(second), 2.0)
        .unwrap();
    game.tick(1.0);
    host.sync(&game.world()).unwrap();
    assert_eq!(
        host.layers()
            .iter()
            .map(|layer| (layer.modifier, layer.strength))
            .collect::<Vec<_>>(),
        vec![(first, 2.0), (second, 0.5)]
    );
    game.world_mut().remove_image_space(first);
    game.world_mut().crossfade_image_space(None, 1.0).unwrap();
    game.tick(1.0);
    host.sync(&game.world()).unwrap();
    assert!(host.layers().is_empty());
    game.world_mut().apply_image_space(first, 1.0).unwrap();
    host.sync(&game.world()).unwrap();
    host.sync(&Game::new(LoadOrder::new()).world()).unwrap();
    assert!(
        host.layers().is_empty(),
        "a replacement world cannot retain the previous stack"
    );
}

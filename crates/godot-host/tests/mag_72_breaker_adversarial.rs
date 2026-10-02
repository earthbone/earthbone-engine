// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::image_space::WeatherSelection;
use soe_game::World;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedBase};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn world_with_sources() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 10)
            .sub(
                Tag::new(b"HNAM"),
                floats(&[0.0, 0.0, 0.0, 0.2, 0.0, 3.0, 1.0, 1.0, 0.25]),
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
                floats(&[0.0, 0.0, 0.0, 0.8, 0.0, 8.0, 1.0, 1.0, 0.75]),
            )
            .sub(Tag::new(b"CNAM"), floats(&[0.4, 1.6, 1.1]))
            .sub(Tag::new(b"TNAM"), floats(&[0.5, 0.1, 0.8, 0.2]))
            .sub(
                Tag::new(b"DNAM"),
                [floats(&[0.8, 600.0, 300.0]), vec![0, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag::new(b"CELL"), 20).sub(
        Tag::new(b"XCIM"),
        10_u32.to_le_bytes(),
    ));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 21));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 22).sub(
        Tag::new(b"XCIM"),
        12_u32.to_le_bytes(),
    ));
    plugin.push_top(Record::new(Tag::new(b"IMGS"), 12).sub(
        Tag::new(b"HNAM"),
        vec![0; 8],
    ));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 30).sub(
        Tag::new(b"IMSP"),
        [10_u32, 11, 0, 0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>(),
    ));
    let mut order = LoadOrder::new();
    order.add("Mag72Breaker.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn mag_72_host_snapshot_clears_when_cell_image_space_source_changes() {
    let mut world = world_with_sources();
    let mut stack = ImageSpaceStack::default();
    world.load_cell(id(20));
    stack.sync(&world).unwrap();
    assert!(stack.base().is_some());

    world.load_cell(id(21));
    stack.sync(&world).unwrap();
    assert!(stack.base().is_none());
}

#[test]
fn mag_72_host_snapshot_clears_after_malformed_replacement_source() {
    let mut world = world_with_sources();
    let mut stack = ImageSpaceStack::default();
    world.load_cell(id(20));
    stack.sync(&world).unwrap();
    assert!(stack.base().is_some());

    world.load_cell(id(22));
    assert!(stack.sync(&world).is_err());
    assert!(stack.base().is_none());
}

#[test]
fn mag_72_weather_snapshot_blends_hdr_color_tint_and_depth_sources() {
    let mut world = world_with_sources();
    world.load_cell(id(20));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    let mut stack = ImageSpaceStack::default();
    stack.sync(&world).unwrap();
    let base = RenderedBase::from(stack.base().unwrap());

    assert_eq!((base.exposure, base.white, base.bloom), (1.5, 5.5, 0.5));
    assert!((base.saturation - 0.5).abs() < f32::EPSILON);
    assert!((base.brightness - 1.4).abs() < 0.00001);
    assert!((base.contrast - 1.25).abs() < f32::EPSILON);
    for (actual, expected) in base.tint.into_iter().zip([0.375, 0.45, 0.5, 0.15]) {
        assert!((actual - expected).abs() < 0.00001);
    }
    assert!((base.depth_amount - 0.65).abs() < 0.00001);
    assert!((base.depth_distance - (500.0 / 70.0)).abs() < 0.00001);
    assert!((base.depth_range - (250.0 / 70.0)).abs() < 0.00001);
}

#[test]
fn mag_72_weather_empty_slots_fall_back_to_current_cell_snapshot() {
    let mut world = world_with_sources();
    world.load_cell(id(20));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 2,
            to: 3,
            blend: 0.5,
        }))
        .unwrap();
    let mut stack = ImageSpaceStack::default();
    stack.sync(&world).unwrap();
    assert_eq!(RenderedBase::from(stack.base().unwrap()).white, 3.0);

    world.load_cell(id(21));
    stack.sync(&world).unwrap();
    assert!(stack.base().is_none());
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_game::world::image_space::WeatherSelection;
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

#[test]
fn mag_72_depth_game_units_convert_to_godot_metres() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 10)
            .sub(
                Tag::new(b"HNAM"),
                floats(&[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0]),
            )
            .sub(
                Tag::new(b"DNAM"),
                [floats(&[1.0, 700.0, 350.0]), vec![0, 0, 0, 64]].concat(),
            ),
    );
    plugin
        .push_top(Record::new(Tag::new(b"CELL"), 20).sub(Tag::new(b"XCIM"), 10_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Depth.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    let mut host = ImageSpaceStack::default();
    host.sync(&world).unwrap();
    let rendered = RenderedBase::from(host.base().unwrap());
    assert_eq!((rendered.depth_distance, rendered.depth_range), (10.0, 5.0));
}

#[test]
fn mag_72_empty_weather_slots_fall_back_to_cell_and_source_change_clears_host() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"IMGS"), 10).sub(
        Tag::new(b"HNAM"),
        floats(&[0.0, 0.0, 0.0, 0.0, 0.0, 7.0, 1.0, 1.0, 0.0]),
    ));
    plugin
        .push_top(Record::new(Tag::new(b"CELL"), 20).sub(Tag::new(b"XCIM"), 10_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 21));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 30).sub(Tag::new(b"IMSP"), vec![0; 16]));
    let mut order = LoadOrder::new();
    order.add("Fallback.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    let mut host = ImageSpaceStack::default();
    host.sync(&world).unwrap();
    assert_eq!(RenderedBase::from(host.base().unwrap()).white, 7.0);
    world.load_cell(id(21));
    host.sync(&world).unwrap();
    assert!(host.base().is_none());
}

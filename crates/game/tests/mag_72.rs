// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::image_space::WeatherSelection;
use soe_game::World;

use crate::common::legacy;

fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn data(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag(*b"IMGS"), 10)
            .sub(
                tag(*b"HNAM"),
                data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]),
            )
            .sub(tag(*b"CNAM"), data(&[0.5, 1.2, 1.4]))
            .sub(tag(*b"TNAM"), data(&[0.25, 0.8, 0.2, 0.1]))
            .sub(
                tag(*b"DNAM"),
                [data(&[0.5, 400.0, 200.0]), vec![1, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(
        Record::new(tag(*b"IMGS"), 11)
            .sub(
                tag(*b"HNAM"),
                data(&[1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 7.0, 8.0, 9.0]),
            )
            .sub(tag(*b"CNAM"), data(&[0.8, 2.0, 1.5]))
            .sub(tag(*b"TNAM"), data(&[0.75, 0.1, 0.7, 0.4]))
            .sub(
                tag(*b"DNAM"),
                [data(&[0.9, 600.0, 300.0]), vec![0, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(Record::new(tag(*b"CELL"), 20).sub(tag(*b"XCIM"), 10_u32.to_le_bytes()));
    plugin.push_top(Record::new(tag(*b"CELL"), 21));
    plugin.push_top(
        Record::new(tag(*b"WTHR"), 30).sub(
            tag(*b"IMSP"),
            [10_u32, 11, 10, 11]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("Images.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    world
}

#[test]
fn mag_72_cell_xcim_resolves_hdr_color_and_depth() {
    let world = fixture();
    let base = world.base_image_space().unwrap().unwrap();
    assert_eq!(base.hdr, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    assert_eq!(base.cinematic, [0.5, 1.2, 1.4]);
    assert_eq!(base.tint, [0.25, 0.8, 0.2, 0.1]);
    assert_eq!(base.depth, [0.5, 400.0, 200.0]);
    assert_eq!(base.depth_flags, 1);
}

#[test]
fn mag_72_weather_imsp_blends_selected_time_and_cleans_up() {
    let mut world = fixture();
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    let base = world.base_image_space().unwrap().unwrap();
    assert_eq!(base.hdr[5], 8.0);
    assert_eq!(base.cinematic[1], 1.6);
    world.select_weather_image_space(None).unwrap();
    assert_eq!(world.base_image_space().unwrap().unwrap().hdr[5], 6.0);
    world.load_cell(id(21));
    assert!(world.base_image_space().unwrap().is_none());
}

#[test]
fn mag_72_invalid_selection_is_atomic_and_saved_selection_round_trips() {
    let mut world = fixture();
    let chosen = WeatherSelection {
        weather: id(30),
        from: 1,
        to: 1,
        blend: 0.0,
    };
    world.select_weather_image_space(Some(chosen)).unwrap();
    let before = world.base_image_space().unwrap();
    assert!(world
        .select_weather_image_space(Some(WeatherSelection {
            blend: f32::NAN,
            ..chosen
        }))
        .is_err());
    assert_eq!(world.base_image_space().unwrap(), before);
    let bytes = world.save_state();
    world.select_weather_image_space(None).unwrap();
    world.load_state(&bytes).unwrap();
    assert_eq!(world.base_image_space().unwrap(), before);
}

#[test]
fn mag_72_malformed_cell_link_returns_an_error() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"CELL"), 20).sub(tag(*b"XCIM"), [10_u8, 0, 0]));
    let mut order = LoadOrder::new();
    order.add("Malformed.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    assert!(world.base_image_space().is_err());
}

#[test]
fn mag_72_non_null_image_space_links_require_live_imgs_records() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"CELL"), 20).sub(tag(*b"XCIM"), 99_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(tag(*b"WTHR"), 30).sub(
            tag(*b"IMSP"),
            [99_u32, 0, 0, 0]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("MissingImages.esm", plugin).unwrap();
    let mut world = World::new(order);

    world.load_cell(id(20));
    assert!(world.base_image_space().is_err());

    world.load_cell(id(21));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    assert!(world.base_image_space().is_err());
}

#[test]
fn mag_72_non_null_image_space_link_rejects_non_imgs_target() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"CELL"), 20).sub(tag(*b"XCIM"), 21_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(tag(*b"WTHR"), 30).sub(
            tag(*b"IMSP"),
            [21_u32, 0, 0, 0]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    plugin.push_top(Record::new(tag(*b"CELL"), 21));
    let mut order = LoadOrder::new();
    order.add("WrongImageType.esm", plugin).unwrap();
    let mut world = World::new(order);

    world.load_cell(id(20));
    assert!(world.base_image_space().is_err());

    world.load_cell(id(22));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    assert!(world.base_image_space().is_err());
}

#[test]
fn mag_72_installed_cell_and_weather_links_resolve() {
    let Ok(path) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let mut order = LoadOrder::new();
    for file in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                file,
                Plugin::parse(&std::fs::read(std::path::Path::new(&path).join(file)).unwrap())
                    .unwrap(),
            )
            .unwrap();
    }
    let cell = order
        .winners_of(tag(*b"CELL"))
        .find(|(_, record)| record.find(tag(*b"XCIM")).is_some())
        .map(|(id, _)| id)
        .unwrap();
    let weather = order
        .winners_of(tag(*b"WTHR"))
        .find(|(_, record)| record.find(tag(*b"IMSP")).is_some())
        .map(|(id, _)| id)
        .unwrap();
    let mut world = World::new(order);
    world.load_cell(cell);
    assert!(world.base_image_space().unwrap().is_some());
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather,
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    assert!(world.base_image_space().unwrap().is_some());
}

#[test]
fn mag_72_absent_section_clears_previous_weather_selection() {
    let mut world = fixture();
    let bytes = world.save_state();
    assert!(!bytes.windows(b"MAG-72".len()).any(|part| part == b"MAG-72"));
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 1,
            to: 1,
            blend: 0.0,
        }))
        .unwrap();
    assert_eq!(world.base_image_space().unwrap().unwrap().hdr[5], 10.0);
    world.load_state(&bytes).unwrap();
    assert_eq!(world.base_image_space().unwrap().unwrap().hdr[5], 6.0);
}

#[test]
fn mag_72_later_section_error_preserves_live_weather_selection() {
    let mut world = fixture();
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 1,
            to: 1,
            blend: 0.0,
        }))
        .unwrap();
    let mut invalid = world.save_state();
    let mut body = invalid.clone();
    legacy::strip_world_sections(&mut body);
    let count_start = body.len();
    let count_end = count_start + std::mem::size_of::<u16>();
    let count = u16::from_le_bytes(invalid[count_start..count_end].try_into().unwrap());
    invalid[count_start..count_end].copy_from_slice(&(count + 1).to_le_bytes());
    invalid.push(3);
    invalid.extend_from_slice(b"bad");
    invalid.extend_from_slice(&0_u32.to_le_bytes());
    world.select_weather_image_space(None).unwrap();
    let before = world.save_state();
    let error = world.load_state(&invalid).unwrap_err();
    assert!(error.to_string().contains("unknown section bad at world"));
    assert_eq!(world.save_state(), before);
}

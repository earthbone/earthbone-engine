// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::image_space::WeatherSelection;
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn image_space_world(cell_link: u32, weather_links: [u32; 4]) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 10)
            .sub(
                Tag::new(b"HNAM"),
                floats(&[0.0, 0.0, 0.0, 0.3, 0.0, 4.0, 1.0, 1.0, 0.2]),
            )
            .sub(Tag::new(b"CNAM"), floats(&[0.6, 1.1, 1.2]))
            .sub(Tag::new(b"TNAM"), floats(&[0.4, 0.2, 0.3, 0.5]))
            .sub(
                Tag::new(b"DNAM"),
                [floats(&[0.5, 400.0, 200.0]), vec![0, 0, 0, 64]].concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag::new(b"CELL"), 20).sub(Tag::new(b"XCIM"), cell_link.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag::new(b"WTHR"), 30).sub(
            Tag::new(b"IMSP"),
            weather_links
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("ImageSpaceAdversarial.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    world
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn mag_72_non_null_cell_xcim_must_resolve_to_an_imgs_record() {
    let world = image_space_world(99, [0; 4]);
    assert!(world.base_image_space().is_err());
}

#[test]
fn mag_72_non_null_weather_imsp_must_resolve_instead_of_falling_back() {
    let mut world = image_space_world(10, [99, 0, 0, 0]);
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

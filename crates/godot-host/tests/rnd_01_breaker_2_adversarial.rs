// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/data_source/terrain_texture.rs"]
mod terrain_texture;

use soe_dds::Image;
use soe_esm::records::land::{Alpha, Land, Layer};

#[test]
fn rnd_01_all_eight_authored_additional_layers_are_composited_in_index_order() {
    let mut land = Land {
        form_id: 1,
        flags: 0,
        heights: None,
        normals: None,
        colours: None,
        quadrants: Default::default(),
        cell: None,
        world: None,
    };
    land.quadrants[0].base = Some(1);
    land.quadrants[0].layers = (0..8)
        .map(|index| Layer {
            texture: index + 2,
            index: index as u16,
            alphas: vec![Alpha {
                position: 0,
                opacity: 1.0,
            }],
        })
        .collect();

    let baked = terrain_texture::bake(&land, |id| {
        Some(Image {
            width: 1,
            height: 1,
            depth: 1,
            rgba: [id as u8, 0, 0, 255].to_vec(),
        })
    });
    let bottom_left = ((256 - 1) * 256) * 4;

    assert_eq!(
        &baked.rgba[bottom_left..bottom_left + 4],
        &[9, 0, 0, 255],
        "LAND supports eight additional layers indexed 0 through 7"
    );
}

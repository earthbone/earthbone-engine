// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/data_source/terrain_texture.rs"]
mod terrain_texture;

use soe_dds::Image;
use soe_esm::records::land::{Alpha, Land, Layer};

fn land() -> Land {
    Land {
        form_id: 1,
        flags: 0,
        heights: None,
        normals: None,
        colours: None,
        quadrants: Default::default(),
        cell: None,
        world: None,
    }
}

fn image(color: [u8; 4]) -> Image {
    Image {
        width: 1,
        height: 1,
        depth: 1,
        rgba: color.to_vec(),
    }
}

fn pixel(image: &Image, x: usize, y: usize) -> [u8; 4] {
    image.rgba[(y * image.width as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

#[test]
fn rnd_01_quadrants_keep_bottom_left_record_order() {
    let mut land = land();
    for (id, quadrant) in land.quadrants.iter_mut().enumerate() {
        quadrant.base = Some(id as u32 + 1);
    }
    let baked = terrain_texture::bake(&land, |id| {
        Some(image([id as u8 * 20, 0, 0, 255]))
    });

    assert_eq!(pixel(&baked, 0, 255), [20, 0, 0, 255]);
    assert_eq!(pixel(&baked, 128, 255), [40, 0, 0, 255]);
    assert_eq!(pixel(&baked, 0, 127), [60, 0, 0, 255]);
    assert_eq!(pixel(&baked, 128, 127), [80, 0, 0, 255]);
}

#[test]
fn rnd_01_zero_base_uses_the_dirt_fallback_and_sparse_alpha_stays_clear() {
    let mut land = land();
    land.quadrants[0].base = Some(0);
    land.quadrants[0].layers.push(Layer {
        texture: 9,
        index: 0,
        alphas: vec![Alpha {
            position: 0,
            opacity: 1.0,
        }],
    });
    let baked = terrain_texture::bake(&land, |id| match id {
        0 => Some(image([10, 20, 30, 255])),
        9 => Some(image([200, 210, 220, 255])),
        _ => None,
    });

    assert_eq!(pixel(&baked, 0, 255), [200, 210, 220, 255]);
    assert_eq!(pixel(&baked, 8, 255), [10, 20, 30, 255]);
    assert_eq!(pixel(&baked, 128, 255), [10, 20, 30, 255]);
    let missing = terrain_texture::bake(&land, |_| None);
    assert_eq!(pixel(&missing, 0, 255), [91, 76, 54, 255]);
}

#[test]
fn rnd_01_alpha_layers_compose_in_layer_index_order() {
    let mut land = land();
    land.quadrants[0].base = Some(1);
    land.quadrants[0].layers.extend([
        Layer {
            texture: 2,
            index: 2,
            alphas: vec![Alpha {
                position: 0,
                opacity: 1.0,
            }],
        },
        Layer {
            texture: 3,
            index: 1,
            alphas: vec![Alpha {
                position: 0,
                opacity: 1.0,
            }],
        },
    ]);
    let baked = terrain_texture::bake(&land, |id| match id {
        1 => Some(image([1, 1, 1, 255])),
        2 => Some(image([2, 2, 2, 255])),
        3 => Some(image([3, 3, 3, 255])),
        _ => None,
    });

    assert_eq!(pixel(&baked, 0, 255), [2, 2, 2, 255]);
}

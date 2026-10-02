// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/data_source/terrain_texture.rs"]
mod terrain_texture;

use soe_dds::Image;
use soe_esm::records::land::{Alpha, Land, Layer};
use soe_godot_host::data_source::{DataSource, TextureData};
use std::collections::HashSet;
use std::path::PathBuf;

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
fn rnd_01_installed_whiterun_painted_surfaces_are_blended_and_nonwhite() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let source =
        DataSource::open(&data).unwrap_or_else(|error| panic!("DataSource::open: {error}"));
    let order = source.shared_order();
    let exterior = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|error| panic!("stream_exterior: {error}"));
    let mut checked = HashSet::new();

    for (_, cell) in &exterior.cells {
        for surface in &cell.surfaces {
            if !surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.starts_with("terrain:"))
                || !checked.insert(surface.reference)
            {
                continue;
            }
            let Some(record) = order.winner(surface.reference) else {
                continue;
            };
            let Ok(land) = Land::from_record(record) else {
                continue;
            };
            if !land
                .quadrants
                .iter()
                .any(|quadrant| quadrant.layers.iter().any(|layer| !layer.alphas.is_empty()))
            {
                continue;
            }
            let Some(TextureData::Rgba8(image)) = surface.textures.diffuse.as_deref() else {
                panic!(
                    "painted LAND {} has no baked RGBA8 texture",
                    surface.reference
                );
            };
            let colors: HashSet<[u8; 3]> = image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            assert!(
                colors.len() > 1,
                "painted LAND {} has no blend",
                surface.reference
            );
            assert!(
                colors.iter().any(|color| *color != [255, 255, 255]),
                "painted LAND {} is all white",
                surface.reference
            );
        }
    }
    assert!(!checked.is_empty(), "no Whiterun terrain surfaces streamed");
}

#[test]
fn rnd_01_vtxt_adjacent_points_interpolate_between_textures() {
    let mut land = land();
    land.quadrants[0].base = Some(10);
    land.quadrants[0].layers.push(Layer {
        texture: 20,
        index: 0,
        alphas: vec![
            Alpha {
                position: 0,
                opacity: 0.0,
            },
            Alpha {
                position: 1,
                opacity: 1.0,
            },
        ],
    });

    let baked = terrain_texture::bake(&land, |id| match id {
        10 => Some(image([0, 0, 0, 255])),
        20 => Some(image([255, 0, 0, 255])),
        _ => None,
    });

    let blended = pixel(&baked, 4, 255);
    assert!(
        (126..=130).contains(&blended[0]),
        "unexpected blend: {blended:?}"
    );
    assert_eq!(&blended[1..], &[0, 0, 255]);
}

#[test]
fn rnd_01_vtxt_layers_composite_in_numeric_layer_order() {
    let mut land = land();
    land.quadrants[0].base = Some(10);
    land.quadrants[0].layers.extend([
        Layer {
            texture: 30,
            index: 2,
            alphas: vec![Alpha {
                position: 0,
                opacity: 0.5,
            }],
        },
        Layer {
            texture: 20,
            index: 1,
            alphas: vec![Alpha {
                position: 0,
                opacity: 1.0,
            }],
        },
    ]);

    let baked = terrain_texture::bake(&land, |id| match id {
        10 => Some(image([0, 0, 0, 255])),
        20 => Some(image([0, 0, 255, 255])),
        30 => Some(image([255, 0, 0, 255])),
        _ => None,
    });

    assert_eq!(pixel(&baked, 0, 255), [128, 0, 128, 255]);
}

#[test]
fn rnd_01_unresolved_ltex_layer_leaves_base_texture_visible() {
    let mut land = land();
    land.quadrants[0].base = Some(10);
    land.quadrants[0].layers.push(Layer {
        texture: 404,
        index: 0,
        alphas: vec![Alpha {
            position: 0,
            opacity: 1.0,
        }],
    });

    let baked = terrain_texture::bake(&land, |id| (id == 10).then(|| image([12, 34, 56, 255])));

    assert_eq!(pixel(&baked, 0, 255), [12, 34, 56, 255]);
}

#[test]
fn rnd_01_zero_base_uses_dirt_then_applies_resolved_alpha_layer() {
    let mut land = land();
    land.quadrants[0].base = Some(0);
    land.quadrants[0].layers.push(Layer {
        texture: 20,
        index: 0,
        alphas: vec![Alpha {
            position: 0,
            opacity: 0.5,
        }],
    });

    let baked = terrain_texture::bake(&land, |id| match id {
        0 => Some(image([100, 80, 60, 255])),
        20 => Some(image([200, 180, 160, 255])),
        _ => None,
    });

    assert_eq!(pixel(&baked, 0, 255), [150, 130, 110, 255]);
}

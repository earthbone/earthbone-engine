// SPDX-License-Identifier: GPL-3.0-or-later

//! Native LAND tiles with separate alpha grids, following xEdit LAND/TXST,
//! UESP LAND and OpenMW terrain/material.cpp. Small weights interpolate
//! independently of diffuse resolution. The old CPU composite remains a test
//! reference for authored layer order and fallback colors.

use soe_dds::Image;
use soe_esm::records::land::{Land, ALPHA_GRID_SIDE};
use std::sync::Arc;

/// Native diffuse tiles and packed alpha grids for one quadrant.
#[derive(Debug, Clone)]
pub struct TerrainQuadrantTextures<T> {
    pub layers: Vec<(String, Arc<T>)>,
    pub weights: Vec<Arc<T>>,
}

/// The four ordered LAND quadrants, independent of texture upload format.
#[derive(Debug, Clone)]
pub struct TerrainTextures<T> {
    pub quadrants: [TerrainQuadrantTextures<T>; 4],
}

#[cfg(test)]
const QUADRANT_PIXELS: usize = 128;
#[cfg(test)]
const IMAGE_SIDE: usize = QUADRANT_PIXELS * 2;
#[cfg(test)]
const TEXTURE_TILES_PER_QUADRANT: f32 = 4.0;
const FALLBACK: [u8; 4] = [91, 76, 54, 255];

pub(super) fn prepare<T>(
    land: &Land,
    mut image_for: impl FnMut(u32) -> Option<(String, Arc<T>)>,
    mut encode_rgba: impl FnMut(Image) -> T,
) -> TerrainTextures<T> {
    TerrainTextures {
        quadrants: std::array::from_fn(|q| {
            let quadrant = &land.quadrants[q];
            let base = quadrant
                .base
                .filter(|id| *id != 0)
                .and_then(&mut image_for)
                .or_else(|| image_for(0))
                .unwrap_or_else(|| {
                    (
                        "terrain:fallback-soil".into(),
                        Arc::new(encode_rgba(Image {
                            width: 1,
                            height: 1,
                            depth: 1,
                            rgba: FALLBACK.to_vec(),
                        })),
                    )
                });
            let mut layers = vec![base];
            let mut overlays: Vec<_> = quadrant
                .layers
                .iter()
                .filter_map(|layer| image_for(layer.texture).map(|image| (layer, image)))
                .collect();
            overlays.sort_by_key(|(layer, _)| layer.index);
            let mut weights = Vec::new();
            for chunk in overlays.chunks(4) {
                let mut rgba = vec![0; ALPHA_GRID_SIDE * ALPHA_GRID_SIDE * 4];
                for (channel, (layer, image)) in chunk.iter().enumerate() {
                    layers.push(image.clone());
                    for alpha in &layer.alphas {
                        if let Some(value) = rgba.get_mut(usize::from(alpha.position) * 4 + channel)
                        {
                            *value = (alpha.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                    }
                }
                weights.push(Arc::new(encode_rgba(Image {
                    width: ALPHA_GRID_SIDE as u32,
                    height: ALPHA_GRID_SIDE as u32,
                    depth: 1,
                    rgba,
                })));
            }
            TerrainQuadrantTextures { layers, weights }
        }),
    }
}

/// Composites four LAND quadrants. Missing texture files use a soil color so
/// terrain remains distinguishable from the host's default white material.
#[cfg(test)]
pub(super) fn bake<I: std::borrow::Borrow<Image>>(
    land: &Land,
    mut image_for: impl FnMut(u32) -> Option<I>,
) -> Image {
    let mut rgba = vec![0; IMAGE_SIDE * IMAGE_SIDE * 4];
    for (quadrant_index, quadrant) in land.quadrants.iter().enumerate() {
        let qx = quadrant_index % 2;
        let qy = quadrant_index / 2;
        let base = quadrant
            .base
            .and_then(|id| (id != 0).then_some(id))
            .and_then(&mut image_for);
        let base = base.or_else(|| image_for(0));
        let mut layers: Vec<_> = quadrant
            .layers
            .iter()
            .filter_map(|layer| image_for(layer.texture).map(|image| (layer, image)))
            .collect();
        layers.sort_by_key(|(layer, _)| layer.index);
        for py in 0..QUADRANT_PIXELS {
            for px in 0..QUADRANT_PIXELS {
                let mut color = base
                    .as_ref()
                    .map(|image| sample(image.borrow(), px, py))
                    .unwrap_or(FALLBACK);
                for (layer, image) in &layers {
                    let opacity = layer_opacity(layer, px, py);
                    let top = sample(image.borrow(), px, py);
                    for channel in 0..3 {
                        color[channel] = (f32::from(color[channel]) * (1.0 - opacity)
                            + f32::from(top[channel]) * opacity)
                            .round() as u8;
                    }
                }
                let x = qx * QUADRANT_PIXELS + px;
                let y = (1 - qy) * QUADRANT_PIXELS + (QUADRANT_PIXELS - 1 - py);
                let offset = (y * IMAGE_SIDE + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    Image {
        width: IMAGE_SIDE as u32,
        height: IMAGE_SIDE as u32,
        depth: 1,
        rgba,
    }
}

#[cfg(test)]
fn sample(image: &Image, x: usize, y: usize) -> [u8; 4] {
    let tx = ((x as f32 / QUADRANT_PIXELS as f32 * TEXTURE_TILES_PER_QUADRANT) * image.width as f32)
        as u32
        % image.width.max(1);
    let ty = ((y as f32 / QUADRANT_PIXELS as f32 * TEXTURE_TILES_PER_QUADRANT)
        * image.height as f32) as u32
        % image.height.max(1);
    let offset = (ty as usize * image.width as usize + tx as usize) * 4;
    [
        image.rgba[offset],
        image.rgba[offset + 1],
        image.rgba[offset + 2],
        image.rgba[offset + 3],
    ]
}

#[cfg(test)]
fn layer_opacity(layer: &soe_esm::records::land::Layer, x: usize, y: usize) -> f32 {
    let mut grid = [0.0f32; ALPHA_GRID_SIDE * ALPHA_GRID_SIDE];
    for alpha in &layer.alphas {
        grid[usize::from(alpha.position)] = alpha.opacity.clamp(0.0, 1.0);
    }
    let gx = x as f32 / (QUADRANT_PIXELS - 1) as f32 * (ALPHA_GRID_SIDE - 1) as f32;
    let gy = y as f32 / (QUADRANT_PIXELS - 1) as f32 * (ALPHA_GRID_SIDE - 1) as f32;
    let x0 = gx.floor() as usize;
    let y0 = gy.floor() as usize;
    let x1 = (x0 + 1).min(ALPHA_GRID_SIDE - 1);
    let y1 = (y0 + 1).min(ALPHA_GRID_SIDE - 1);
    let fx = gx - x0 as f32;
    let fy = gy - y0 as f32;
    let at = |row: usize, col: usize| grid[row * ALPHA_GRID_SIDE + col];
    let south = at(y0, x0) * (1.0 - fx) + at(y0, x1) * fx;
    let north = at(y1, x0) * (1.0 - fx) + at(y1, x1) * fx;
    (south * (1.0 - fy) + north * fy).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::records::land::{Alpha, Layer};

    fn image(color: [u8; 4]) -> Arc<Image> {
        Arc::new(Image {
            width: 1,
            height: 1,
            depth: 1,
            rgba: color.to_vec(),
        })
    }

    #[test]
    fn native_tiles_keep_source_resolution_and_order_separate_from_weights() {
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
        land.quadrants[0].base = Some(10);
        for index in (0..5).rev() {
            land.quadrants[0].layers.push(Layer {
                texture: 20 + u32::from(index),
                index,
                alphas: vec![Alpha {
                    position: 0,
                    opacity: f32::from(index + 1) / 5.0,
                }],
            });
        }
        let source = Arc::new(Image {
            width: 1024,
            height: 1024,
            depth: 1,
            rgba: vec![64; 1024 * 1024 * 4],
        });
        let terrain = prepare(
            &land,
            |id| Some((format!("tile-{id}"), Arc::clone(&source))),
            std::convert::identity,
        );
        let quadrant = &terrain.quadrants[0];
        assert_eq!(quadrant.layers.len(), 6);
        assert_eq!(quadrant.layers[0].0, "tile-10");
        assert_eq!(quadrant.layers[1].0, "tile-20");
        assert_eq!(quadrant.layers[5].0, "tile-24");
        assert!(quadrant
            .layers
            .iter()
            .all(|(_, image)| Arc::ptr_eq(image, &source)));
        assert_eq!(quadrant.layers[0].1.width, 1024);
        assert_eq!(quadrant.weights.len(), 2);
        let first = quadrant.weights[0].as_ref();
        let second = quadrant.weights[1].as_ref();
        assert_eq!((first.width, first.height), (17, 17));
        assert_eq!(&first.rgba[..4], &[51, 102, 153, 204]);
        assert_eq!(&second.rgba[..4], &[255, 0, 0, 0]);
        assert!(first.rgba[4..].iter().all(|value| *value == 0));
    }

    #[test]
    fn rnd_01_alpha_layer_blends_with_quadrant_base() {
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
        land.quadrants[0].base = Some(10);
        land.quadrants[0].layers.push(Layer {
            texture: 20,
            index: 0,
            alphas: vec![Alpha {
                position: 0,
                opacity: 1.0,
            }],
        });
        let baked = bake(&land, |id| match id {
            10 => Some(image([20, 30, 40, 255])),
            20 => Some(image([200, 150, 100, 255])),
            _ => None,
        });
        let southwest = ((IMAGE_SIDE - 1) * IMAGE_SIDE) * 4;
        assert_eq!(&baked.rgba[southwest..southwest + 4], &[200, 150, 100, 255]);
        let southeast = (IMAGE_SIDE - 1) * IMAGE_SIDE * 4 + QUADRANT_PIXELS * 4;
        assert_eq!(&baked.rgba[southeast..southeast + 4], &FALLBACK);
    }
}

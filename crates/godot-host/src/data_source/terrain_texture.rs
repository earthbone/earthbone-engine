// SPDX-License-Identifier: GPL-3.0-or-later

//! Bakes Skyrim LAND quadrant layers into a sampled RGBA image for the host
//! terrain mesh. Record layout follows TES5Edit's LAND and TXST definitions
//! and the UESP LAND description. Godot's renderer receives the same single
//! albedo texture path as an ordinary surface.

use soe_dds::Image;
use soe_esm::records::land::{Land, ALPHA_GRID_SIDE};

const QUADRANT_PIXELS: usize = 128;
const IMAGE_SIDE: usize = QUADRANT_PIXELS * 2;
const TEXTURE_TILES_PER_QUADRANT: f32 = 4.0;
const FALLBACK: [u8; 4] = [91, 76, 54, 255];

/// Composites four LAND quadrants. Missing texture files use a soil color so
/// terrain remains distinguishable from the host's default white material.
pub(super) fn bake(land: &Land, mut image_for: impl FnMut(u32) -> Option<Image>) -> Image {
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
                    .map(|image| sample(image, px, py))
                    .unwrap_or(FALLBACK);
                for (layer, image) in &layers {
                    let opacity = layer_opacity(layer, px, py);
                    let top = sample(image, px, py);
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

    fn image(color: [u8; 4]) -> Image {
        Image {
            width: 1,
            height: 1,
            depth: 1,
            rgba: color.to_vec(),
        }
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

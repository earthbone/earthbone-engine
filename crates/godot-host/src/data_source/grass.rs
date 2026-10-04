// SPDX-License-Identifier: GPL-3.0-or-later

//! Procedural grass placement, driven by a cell's `LAND` texture layers.
//!
//! Vanilla never places grass through a `REFR`. Each `LAND` quadrant paints
//! `LTEX` textures over a 17 by 17 alpha grid (`land.rs`), an `LTEX` names
//! the `GRAS` records it may grow through a repeated `GNAM` (`ltex.rs`), and
//! the engine scatters instances over the painted area, filtered by each
//! `GRAS` record's density and slope range. No corpus source read for this
//! (`TES5Edit`, `Mutagen`, OpenMW's `groundcoverstore.cpp`, which implements
//! a different, non `ESM4` system) publishes the exact vanilla scatter
//! pattern, so this places on a deterministic hashed grid instead of
//! reproducing it bit for bit. See `docs/godot-host.md`.
//!
//! Every input to the placement hash is a stable coordinate: the cell's
//! world grid position, quadrant, height grid cell and instance index. No
//! wall clock and no global random state, so streaming the same cell twice
//! produces the same batch, which is what the release-on-unload path and
//! the RND-11 done check both rely on.
//!
//! Only the alpha layers (`ATXT`/`VTXT`) are read as grass sources. A
//! quadrant's base texture (`BTXT`) can also carry a `GNAM` list, but its
//! effective opacity is whatever the alpha layers leave uncovered rather
//! than a value the record states outright, so it is left out of this pass.
//! `docs/godot-host.md` carries this as an open gap.

use std::collections::HashMap;
use std::sync::Arc;

use soe_esm::records::gras::Grass;
use soe_esm::records::land::{Land, ALPHA_GRID_SIDE, VERTS_PER_SIDE};
use soe_esm::records::ltex::LandTexture;
use soe_esm::records::refr::Refr;
use soe_esm::records::Transform;
use soe_esm::{tag, GlobalFormId};

use super::{DataSource, ExteriorCell, LoadedStatic, LoadedSurface};

/// Hard cap on placements drawn from one height grid quad, so a high
/// density value cannot make one quad's batch unbounded. The divisor in
/// `instance_count` keeps ordinary density values well under this already,
/// this is the backstop for an edited or unusually dense `GRAS` record.
const MAX_INSTANCES_PER_QUAD: usize = 6;

/// Hard cap on total grass placements per cell, the "bounded" in RND-11's
/// title. Each placement reparses its model's NIF, the same per reference
/// cost every other static already pays in `load_static_surfaces`, so this
/// also keeps a densely painted cell's streaming time reasonable until
/// placements share one decoded mesh instead of one each.
pub(super) const MAX_GRASS_PLACEMENTS_PER_CELL: usize = 512;

/// Quadrant row and column offsets into the cell's 33 by 33 height grid.
/// `Land::quadrants` is documented "Bottom left, bottom right, top left,
/// top right", and each quadrant's 17 by 17 alpha grid spans 16 quads, so
/// quadrant 1 starts 16 columns in and quadrant 2 starts 16 rows in.
const QUADRANT_OFFSETS: [(usize, usize); 4] = [(0, 0), (0, 16), (16, 0), (16, 16)];

/// SplitMix64's finalizer (Sebastiano Vigna, public domain), used only to
/// mix small integer coordinates into one well distributed value. No
/// generator state carried between calls, so the same input always mixes
/// to the same output.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x
}

/// One placement's seed, from coordinates that are the same every time this
/// cell streams: the cell's own grid position, the quadrant, the height
/// grid cell inside it, and which instance of that cell this is.
fn seed(cell: &ExteriorCell, quadrant: usize, row: usize, col: usize, instance: usize) -> u64 {
    let a = mix((cell.x as u32 as u64) | (cell.y as u32 as u64) << 32);
    let b = mix(a ^ quadrant as u64);
    let c = mix(b ^ ((row as u64) << 32 | col as u64));
    mix(c ^ instance as u64)
}

/// A `[0, 1)` value from a seed, using its top 24 bits, which is exact in
/// `f32` and plenty for placement jitter. Not for cryptographic use.
fn unit_fraction(seed: u64) -> f32 {
    ((seed >> 40) as u32 as f32) / (1u32 << 24) as f32
}

/// Bilinear height inside one height grid quad. `row`/`col` is the quad's
/// south west corner, `fx`/`fy` range `0` to `1` across it, matching how
/// `terrain_surface_from_heights` in `exterior.rs` lays the same grid out.
fn bilinear_height(heights: &[f32], row: usize, col: usize, fx: f32, fy: f32) -> f32 {
    let at = |r: usize, c: usize| heights[r * VERTS_PER_SIDE + c];
    let sw = at(row, col);
    let se = at(row, col + 1);
    let nw = at(row + 1, col);
    let ne = at(row + 1, col + 1);
    let south = sw + (se - sw) * fx;
    let north = nw + (ne - nw) * fx;
    south + (north - south) * fy
}

/// Slope at a height grid quad, in degrees off horizontal, from its two
/// edge gradients. An approximation of the quad's actual triangle normals,
/// cheap enough to run per candidate placement and close enough to gate
/// grass off obviously steep or vertical ground.
fn quad_slope_degrees(heights: &[f32], row: usize, col: usize, step: f32) -> f32 {
    let at = |r: usize, c: usize| heights[r * VERTS_PER_SIDE + c];
    let dx = (at(row, col + 1) - at(row, col)) / step;
    let dy = (at(row + 1, col) - at(row, col)) / step;
    dx.hypot(dy).atan().to_degrees()
}

/// Instances to draw from one quad, from the `GRAS` record's density and
/// the layer's local opacity. The CK density slider has no published
/// scatter curve, so this divides it down to a small per-quad count rather
/// than trying to match vanilla's actual density, and `MAX_INSTANCES_PER_QUAD`
/// backstops an edited or unusually dense record.
fn instance_count(density: u8, opacity: f32) -> usize {
    let scaled = (f32::from(density) * opacity.clamp(0.0, 1.0)) / 25.0;
    (scaled.round() as usize).min(MAX_INSTANCES_PER_QUAD)
}

impl DataSource {
    /// Grass placements for one exterior cell's `LAND`, already built and
    /// ready to append to that cell's `LoadedCell::surfaces`. `land_id` is
    /// the winning `LAND` record's own id, its owning file is what the
    /// quadrant texture FormIDs and, downstream, `LTEX` `GNAM` FormIDs
    /// resolve against. `heights` is the cell's own decoded, seam
    /// reconciled height grid, the same array `stream_exterior_each` builds
    /// for the terrain surface. Placements are collected in a fixed order
    /// (quadrant, row, column, layer, `GNAM` entry, instance) before any
    /// model is built, so the surface order is the same on every stream.
    pub(super) fn grass_surfaces_for_cell(
        &self,
        cell: &ExteriorCell,
        land_id: GlobalFormId,
        land: &Land,
        heights: &[f32],
    ) -> GrassBuild {
        let Some(land_file) = self.order().winner_ref(land_id).map(|r| r.file) else {
            return GrassBuild::default();
        };
        let step = crate::units::CELL_UNITS / (VERTS_PER_SIDE - 1) as f32;
        let mut work: Vec<(String, Refr, GlobalFormId)> = Vec::new();

        'cell: for (q, quadrant) in land.quadrants.iter().enumerate() {
            let (row_off, col_off) = QUADRANT_OFFSETS[q];
            for local_row in 0..ALPHA_GRID_SIDE - 1 {
                for local_col in 0..ALPHA_GRID_SIDE - 1 {
                    let position = (local_row * ALPHA_GRID_SIDE + local_col) as u16;
                    let row = row_off + local_row;
                    let col = col_off + local_col;
                    for layer in &quadrant.layers {
                        let Some(alpha) = layer.alphas.iter().find(|a| a.position == position)
                        else {
                            continue;
                        };
                        if alpha.opacity <= 0.0 {
                            continue;
                        }
                        let texture_id = self.order().resolve(land_file, layer.texture);
                        let Some(texture_placement) = self.order().winner_ref(texture_id) else {
                            continue;
                        };
                        let texture_record = self.order().record(texture_placement);
                        if texture_record.tag != tag::LTEX {
                            continue;
                        }
                        let Ok(ltex) = LandTexture::from_record(texture_record) else {
                            continue;
                        };
                        for &grass_raw in &ltex.grasses {
                            let grass_id = self.order().resolve(texture_placement.file, grass_raw);
                            let Some(grass_placement) = self.order().winner_ref(grass_id) else {
                                continue;
                            };
                            let grass_record = self.order().record(grass_placement);
                            if grass_record.tag != tag::GRAS {
                                continue;
                            }
                            let Ok(grass) = Grass::from_record(grass_record) else {
                                continue;
                            };
                            let Some(model) =
                                grass.model.as_ref().filter(|m| !m.is_empty()).cloned()
                            else {
                                continue;
                            };
                            let slope = quad_slope_degrees(heights, row, col, step);
                            if !grass.fits_slope(slope) {
                                continue;
                            }
                            let count = instance_count(grass.density, alpha.opacity);
                            for instance in 0..count {
                                if work.len() >= MAX_GRASS_PLACEMENTS_PER_CELL {
                                    break 'cell;
                                }
                                let s = seed(cell, q, row, col, instance);
                                let jitter_x = unit_fraction(mix(s ^ 1));
                                let jitter_y = unit_fraction(mix(s ^ 2));
                                let yaw_u = unit_fraction(mix(s ^ 3));
                                let scale_u = unit_fraction(mix(s ^ 4));
                                let fx =
                                    (0.5 + (jitter_x - 0.5) * grass.position_range).clamp(0.0, 1.0);
                                let fy =
                                    (0.5 + (jitter_y - 0.5) * grass.position_range).clamp(0.0, 1.0);
                                let yaw = yaw_u * std::f32::consts::TAU;
                                let scale = (1.0 + (scale_u - 0.5) * grass.height_range).max(0.05);
                                let base_x = cell.x as f32 * crate::units::CELL_UNITS
                                    + (col as f32 + fx) * step;
                                let base_y = cell.y as f32 * crate::units::CELL_UNITS
                                    + (row as f32 + fy) * step;
                                let z = bilinear_height(heights, row, col, fx, fy);
                                let refr = Refr {
                                    form_id: 0,
                                    record_flags: 0,
                                    editor_id: None,
                                    base: 0,
                                    transform: Some(Transform {
                                        position: [base_x, base_y, z],
                                        rotation: [0.0, 0.0, yaw],
                                    }),
                                    scale,
                                    enable_parent: None,
                                    owner: None,
                                    count: None,
                                    teleport: None,
                                    lock: None,
                                    location: None,
                                    layer: None,
                                    encounter_zone: None,
                                    emittance: None,
                                    linked_refs: Vec::new(),
                                    primitive: None,
                                    cell: None,
                                    world: None,
                                    in_persistent_group: false,
                                };
                                work.push((model.clone(), refr, grass_id));
                            }
                        }
                    }
                }
            }
        }
        self.build_grass(&work)
    }

    /// Builds the collected placements through the same `build_all` path a
    /// cell's statics take, so the per-cell `INSTANCE_MIN_PLACEMENTS` count
    /// decides instancing here too. A model placed fewer times than that
    /// keeps its placement baked into its vertices, the way a rarely used
    /// static does, since `instancing_candidates` hands short groups back to
    /// the merged batches, which expect world-space geometry.
    fn build_grass(&self, work: &[(String, Refr, GlobalFormId)]) -> GrassBuild {
        let mut model_counts: HashMap<String, usize> = HashMap::new();
        for (model, _, _) in work {
            *model_counts.entry(model.to_lowercase()).or_insert(0) += 1;
        }
        let results = match self.build_all(work, &model_counts) {
            Ok(results) => results,
            Err(e) => {
                soe_trace::warn!(target: "host::grass", system = "host::grass", "stream_exterior: grass build: {e}");
                return GrassBuild::default();
            }
        };
        let mut build = GrassBuild::default();
        for ((model, refr, grass_id), result) in work.iter().zip(results) {
            match result {
                Ok(LoadedStatic::Surfaces {
                    built,
                    instance_transform,
                    ..
                }) => {
                    build.placed += 1;
                    let placement = super::placement_iso(refr);
                    let identity_base = instance_transform.is_some().then(|| model.to_lowercase());
                    for (shape_index, (arrays, textures)) in built.into_iter().enumerate() {
                        let mesh_identity = identity_base
                            .as_ref()
                            .map(|base| Arc::<str>::from(format!("{base}#{shape_index}")));
                        build.surfaces.push(LoadedSurface {
                            reference: *grass_id,
                            arrays,
                            textures,
                            placement,
                            instance_transform,
                            mesh_identity,
                        });
                    }
                }
                Err(e) => {
                    soe_trace::warn!(target: "host::grass", system = "host::grass", "stream_exterior: grass {grass_id}: {e}");
                    build.failed += 1;
                }
            }
        }
        build
    }
}

/// One cell's grass, built and ready to append to its `LoadedCell`.
#[derive(Default)]
pub(super) struct GrassBuild {
    /// Surfaces for every placement that built, one per model shape.
    pub(super) surfaces: Vec<LoadedSurface>,
    /// Placements whose model built, which can differ from the surface
    /// count when a grass model has more than one shape.
    pub(super) placed: usize,
    /// Placements whose model failed to read or parse.
    pub(super) failed: usize,
}

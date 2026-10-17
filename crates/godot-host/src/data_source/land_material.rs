// SPDX-License-Identifier: GPL-3.0-or-later

//! LAND collision materials from xEdit LAND, LTEX MNAM and MATT MNAM.
//! UESP LAND defines the 33 by 33 lattice and ordered quadrant alpha layers.
//! OpenMW's ESM4 loadltex.cpp confirms MNAM is a material FormID in Skyrim.
//! Its renderer does not expose this host's collision metadata, so we attach
//! the strongest composited material at each surviving triangle's centre.

use super::{DataSource, DataSourceError};
use soe_esm::cursor::Fields;
use soe_esm::records::land::{Land, ALPHA_GRID_SIDE, VERTS_PER_SIDE};
use soe_esm::{GlobalFormId, Tag};
use soe_game::footsteps::{material_name_hash, Footsteps};

impl DataSource {
    /// Material of water above a contact in Skyrim coordinates, if this cell has water.
    pub fn water_material_at(&self, cell: GlobalFormId, point: [f32; 3]) -> Option<u32> {
        let cell = if let Some(world) = self.exterior_world(cell) {
            self.cell_index().exterior_cell(
                world,
                (point[0] / crate::units::CELL_UNITS).floor() as i32,
                (point[1] / crate::units::CELL_UNITS).floor() as i32,
            )?
        } else {
            cell
        };
        water_material(self.order(), cell, point[2])
    }

    pub(super) fn land_triangle_materials(
        &self,
        id: GlobalFormId,
        indices: &[i32],
    ) -> Result<Vec<Option<u32>>, DataSourceError> {
        let Some(owner) = self.order().winner_ref(id) else {
            return Ok(vec![None; indices.len() / 3]);
        };
        let land = Land::from_record(self.order().record(owner))?;
        let fallback = Footsteps::new(self.order()).default_object(*b"DLMT")?;
        let hash_for = |raw| -> soe_esm::Result<Option<u32>> {
            let material = if raw == 0 {
                fallback
            } else {
                let ltex = self.order().resolve(owner.file, raw);
                let Some(ltex_owner) = self.order().winner_ref(ltex) else {
                    return Ok(None);
                };
                let record = self.order().record(ltex_owner);
                if record.tag != Tag(*b"LTEX") {
                    return Ok(None);
                }
                let Some(data) = record.find(Tag(*b"MNAM")) else {
                    return Ok(None);
                };
                let mut field = Fields::new(record.tag, Tag(*b"MNAM"), data);
                field.exact(4)?;
                let raw = field.u32()?;
                (raw != 0).then(|| self.order().resolve(ltex_owner.file, raw))
            };
            Ok(material
                .and_then(|id| self.order().winner(id))
                .filter(|record| record.tag == Tag(*b"MATT") && !record.is_deleted())
                .and_then(|record| record.find(Tag(*b"MNAM")))
                .map(material_name_hash))
        };
        let mut quadrant_materials = Vec::new();
        for quadrant in &land.quadrants {
            let mut layers: Vec<_> = quadrant.layers.iter().collect();
            layers.sort_by_key(|layer| layer.index);
            let base = hash_for(quadrant.base.unwrap_or(0))?;
            let mut painted = Vec::new();
            for layer in layers {
                let mut grid = [0.0_f32; ALPHA_GRID_SIDE * ALPHA_GRID_SIDE];
                for alpha in &layer.alphas {
                    grid[usize::from(alpha.position)] = alpha.opacity.clamp(0.0, 1.0);
                }
                painted.push((hash_for(layer.texture)?, grid));
            }
            quadrant_materials.push((base, painted));
        }
        Ok(indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|triangle| {
                let x = triangle
                    .iter()
                    .map(|i| *i as usize % VERTS_PER_SIDE)
                    .sum::<usize>() as f32
                    / 3.0;
                let y = triangle
                    .iter()
                    .map(|i| *i as usize / VERTS_PER_SIDE)
                    .sum::<usize>() as f32
                    / 3.0;
                let qx = usize::from(x >= 16.0);
                let qy = usize::from(y >= 16.0);
                let (base, layers) = &quadrant_materials[qy * 2 + qx];
                let x = x - qx as f32 * 16.0;
                let y = y - qy as f32 * 16.0;
                let mut weights = vec![(*base, 1.0)];
                for (material, grid) in layers {
                    let alpha = opacity(grid, x, y);
                    for (_, weight) in &mut weights {
                        *weight *= 1.0 - alpha;
                    }
                    if let Some((_, weight)) = weights.iter_mut().find(|(id, _)| id == material) {
                        *weight += alpha;
                    } else {
                        weights.push((*material, alpha));
                    }
                }
                weights
                    .into_iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .and_then(|entry| entry.0)
            })
            .collect())
    }
}

fn water_material(order: &soe_esm::LoadOrder, cell: GlobalFormId, height: f32) -> Option<u32> {
    let owner = order.winner_ref(cell)?;
    let record = order.record(owner);
    let cell = soe_esm::records::cell::Cell::from_record(
        record,
        order.file(owner.file).plugin.header.is_localized(),
    )
    .ok()?;
    if !cell.has_water() {
        return None;
    }
    let inherited = cell
        .world
        .map(|world| super::water::world_water(order, order.resolve(owner.file, world)))
        .unwrap_or_default();
    let level = cell
        .water_height
        .filter(|height| super::water::is_recorded_height(*height))
        .or(inherited.height)?;
    if height >= level {
        return None;
    }
    let water = cell
        .water
        .map(|water| order.resolve(owner.file, water))
        .or(inherited.water)?;
    let owner = order.winner_ref(water)?;
    let record = order.record(owner);
    let mut field = Fields::new(record.tag, Tag(*b"TNAM"), record.find(Tag(*b"TNAM"))?);
    field.exact(4).ok()?;
    let raw = field.u32().ok()?;
    if raw == 0 {
        return None;
    }
    let material = order.winner(order.resolve(owner.file, raw))?;
    material.find(Tag(*b"MNAM")).map(material_name_hash)
}

fn opacity(grid: &[f32; ALPHA_GRID_SIDE * ALPHA_GRID_SIDE], x: f32, y: f32) -> f32 {
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(ALPHA_GRID_SIDE - 1);
    let y1 = (y0 + 1).min(ALPHA_GRID_SIDE - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let south = grid[y0 * ALPHA_GRID_SIDE + x0] * (1.0 - fx) + grid[y0 * ALPHA_GRID_SIDE + x1] * fx;
    let north = grid[y1 * ALPHA_GRID_SIDE + x0] * (1.0 - fx) + grid[y1 * ALPHA_GRID_SIDE + x1] * fx;
    south * (1.0 - fy) + north * fy
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    #[test]
    fn r4_submerged_foot_uses_water_material_and_dry_foot_keeps_ground() {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag(*b"CELL"), 1)
                .sub(b"DATA", [3, 0])
                .sub(b"XCLW", 10.0_f32.to_le_bytes())
                .sub(b"XCWT", 2_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"WATR"), 2).sub(b"TNAM", 3_u32.to_le_bytes()));
        plugin.push_top(Record::new(Tag(*b"MATT"), 3).zsub(b"MNAM", "Water"));
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let cell = GlobalFormId { file: 0, object: 1 };
        assert_eq!(
            water_material(&order, cell, 9.0),
            Some(material_name_hash(b"Water"))
        );
        assert_eq!(water_material(&order, cell, 10.0), None);
        assert_eq!(water_material(&order, cell, 11.0), None);
    }
}

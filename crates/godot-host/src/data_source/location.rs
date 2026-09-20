// SPDX-License-Identifier: GPL-3.0-or-later

//! Host resources for a logical player destination. Exterior persistent cells
//! identify a worldspace, while the landing position chooses its grid cells.
//! CellIndex and the existing parent LAND path supply the same record semantics
//! used by the exterior renderer. Logical reference ownership is left intact.

use std::collections::HashMap;

use soe_esm::{tag, GlobalFormId};
use soe_physics::{Collider, Iso, PhysicsWorld, Shape, SimpleWorld, Vec3};

use super::{DataSource, DataSourceError, ExteriorBlock, ExteriorCell, LoadedCell};
use crate::collision::{CellCollision, CollisionStats};
use crate::units::{GodotPos, CELL_UNITS};

pub struct LoadedLocation {
    pub cells: Vec<(GlobalFormId, LoadedCell)>,
    pub collision: CellCollision,
}

impl DataSource {
    /// Prepare visible geometry and matching collision for a logical destination.
    /// The exterior radius is bounded to prevent an accidental unbounded load.
    pub fn load_location(
        &self,
        cell: GlobalFormId,
        position: [f32; 3],
        radius: i32,
    ) -> Result<LoadedLocation, DataSourceError> {
        if !(0..=4).contains(&radius) || position.iter().any(|v| !v.is_finite()) {
            return Err(DataSourceError::CellNotFound(
                "invalid destination coordinates or radius".into(),
            ));
        }
        let index = self.cell_index();
        let cells = if index.is_interior(cell) {
            vec![(cell, self.stream_cell_id(cell)?)]
        } else {
            let owner = self
                .order
                .winner_ref(cell)
                .ok_or_else(|| DataSourceError::CellNotFound(cell.to_string()))?;
            let record = self.order.record(owner);
            if record.tag != tag::CELL {
                return Err(DataSourceError::CellNotFound(cell.to_string()));
            }
            let world = record
                .placement
                .world
                .map(|raw| self.order.resolve(owner.file, raw))
                .ok_or_else(|| {
                    DataSourceError::CellNotFound(format!("{cell} has no worldspace"))
                })?;
            let origin = (
                (position[0] / CELL_UNITS).floor() as i32,
                (position[1] / CELL_UNITS).floor() as i32,
            );
            let select = |world| {
                let mut cells = Vec::new();
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let (Some(x), Some(y)) =
                            (origin.0.checked_add(dx), origin.1.checked_add(dy))
                        else {
                            continue;
                        };
                        if let Some(id) = index.exterior_cell(world, x, y) {
                            cells.push(ExteriorCell { x, y, id });
                        }
                    }
                }
                cells
            };
            let land_parent = self.land_parent_of(world);
            let block = ExteriorBlock {
                world,
                world_editor_id: self
                    .order
                    .winner(world)
                    .and_then(soe_esm::Record::editor_id),
                origin,
                radius,
                cells: select(world),
                land_parent,
                parent_cells: land_parent.map(select).unwrap_or_default(),
            };
            if !block.cells.iter().any(|c| (c.x, c.y) == origin) {
                return Err(DataSourceError::CellNotFound(format!(
                    "{world} at {origin:?}"
                )));
            }
            let mut cells: Vec<_> = self
                .stream_exterior_block(block)?
                .cells
                .into_iter()
                .map(|(cell, loaded)| (cell.id, loaded))
                .collect();
            if let Some(persistent) = index.world_persistent_cell(world) {
                if !cells.iter().any(|(id, _)| *id == persistent) {
                    cells.push((persistent, self.stream_cell_id(persistent)?));
                }
            }
            cells
        };
        let mut world = SimpleWorld::new();
        let mut references = HashMap::new();
        let mut stats = CollisionStats::default();
        for (id, loaded) in &cells {
            let (placed, found) = self.placed_cell_colliders(*id)?;
            stats.references += found.references;
            stats.with_collision += found.with_collision;
            stats.colliders += found.colliders;
            stats.failed += found.failed;
            stats.skipped_blocks += found.skipped_blocks;
            for placed in placed.into_iter().filter(|p| p.collider.is_static()) {
                let before = world.primitive_count();
                let body = world.add_static(&placed.collider);
                references.insert(body, placed.reference);
                stats.bodies += 1;
                stats.primitives += world.primitive_count() - before;
            }
            for surface in &loaded.surfaces {
                if self
                    .order
                    .winner(surface.reference)
                    .is_none_or(|r| r.tag != tag::LAND)
                {
                    continue;
                }
                let arrays = &surface.arrays;
                let collider = Collider {
                    node: soe_nif::BlockRef(-1),
                    node_name: String::new(),
                    layer: soe_physics::bhk::Layer::STATIC,
                    motion: soe_physics::bhk::MotionType::Fixed,
                    mass: 0.0,
                    transform: Iso::IDENTITY,
                    shape: Shape::TriangleMesh {
                        materials: Vec::new(),
                        vertices: arrays
                            .positions
                            .iter()
                            .map(|p| {
                                let p = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                                Vec3::new(p.x, p.y, p.z)
                            })
                            .collect(),
                        // SurfaceArrays uses Godot's clockwise front faces.
                        triangles: arrays
                            .indices
                            .chunks_exact(3)
                            .map(|t| [t[0] as u32, t[2] as u32, t[1] as u32])
                            .collect(),
                    },
                };
                let before = world.primitive_count();
                let body = world.add_static(&collider);
                references.insert(body, surface.reference);
                stats.bodies += 1;
                stats.primitives += world.primitive_count() - before;
            }
        }
        Ok(LoadedLocation {
            cells,
            collision: CellCollision {
                world,
                stats,
                references,
            },
        })
    }
}

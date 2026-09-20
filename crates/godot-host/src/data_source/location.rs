// SPDX-License-Identifier: GPL-3.0-or-later

//! Host resources for a logical player destination. Exterior persistent cells
//! identify a worldspace, while the landing position chooses its grid cells.
//! CellIndex and the existing parent LAND path supply the same record semantics
//! used by the exterior renderer. Logical reference ownership is left intact.

use std::collections::{BTreeMap, HashMap};

use soe_esm::{tag, GlobalFormId};
use soe_physics::{BodyId, Collider, Iso, PhysicsWorld, Shape, SimpleWorld, Vec3};

use super::{DataSource, DataSourceError, ExteriorBlock, ExteriorCell, LoadedCell};
use crate::collision::{CellCollision, CollisionStats, PlacedCollider};
#[cfg(feature = "godot")]
use crate::streaming::CellCoord;
use crate::units::{GodotPos, CELL_UNITS};

pub struct LoadedLocation {
    pub cells: Vec<(GlobalFormId, LoadedCell)>,
    pub collision: CellCollision,
    /// Host body ownership, including parent-world terrain on the local cell.
    pub collision_cells: BTreeMap<GlobalFormId, Vec<BodyId>>,
}

#[cfg(feature = "godot")]
pub(crate) struct PreparedCell {
    pub id: GlobalFormId,
    pub loaded: LoadedCell,
    pub colliders: Vec<PlacedCollider>,
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
        let mut colliders = Vec::new();
        let mut world = SimpleWorld::new();
        let mut references = HashMap::new();
        let mut collision_cells = BTreeMap::new();
        let mut stats = CollisionStats::default();
        for (id, loaded) in &cells {
            let (placed, found) = self.location_colliders(*id, loaded)?;
            stats.references += found.references;
            stats.with_collision += found.with_collision;
            stats.colliders += found.colliders;
            stats.failed += found.failed;
            stats.skipped_blocks += found.skipped_blocks;
            let mut bodies = Vec::new();
            for placed in placed.into_iter().filter(|p| p.collider.is_static()) {
                let before = world.primitive_count();
                let body = world.add_static(&placed.collider);
                references.insert(body, placed.reference);
                bodies.push(body);
                colliders.push(placed.collider);
                stats.bodies += 1;
                stats.primitives += world.primitive_count() - before;
            }
            collision_cells.insert(*id, bodies);
        }
        Ok(LoadedLocation {
            cells,
            collision_cells,
            collision: CellCollision {
                colliders,
                world,
                stats,
                references,
            },
        })
    }

    /// Worldspace ownership comes from the winning CELL's own file namespace.
    #[cfg(feature = "godot")]
    pub(crate) fn exterior_world(&self, cell: GlobalFormId) -> Option<GlobalFormId> {
        if self.cell_index().is_interior(cell) {
            return None;
        }
        let owner = self.order.winner_ref(cell)?;
        let record = self.order.record(owner);
        if record.tag != tag::CELL || record.is_deleted() {
            return None;
        }
        record
            .placement
            .world
            .map(|raw| self.order.resolve(owner.file, raw))
    }

    /// Prepare only the requested grid cells. Missing border cells have no resources.
    #[cfg(feature = "godot")]
    pub(crate) fn prepare_grid_cells(
        &self,
        world: GlobalFormId,
        coordinates: &[CellCoord],
    ) -> Result<Vec<PreparedCell>, DataSourceError> {
        if coordinates.is_empty() {
            return Ok(Vec::new());
        }
        let index = self.cell_index();
        let select = |world| {
            coordinates
                .iter()
                .filter_map(|c| {
                    index.exterior_cell(world, c.x, c.y).map(|id| ExteriorCell {
                        x: c.x,
                        y: c.y,
                        id,
                    })
                })
                .collect()
        };
        let land_parent = self.land_parent_of(world);
        let block = ExteriorBlock {
            world,
            world_editor_id: None,
            origin: coordinates.first().map_or((0, 0), |c| (c.x, c.y)),
            radius: 0,
            cells: select(world),
            land_parent,
            parent_cells: land_parent.map(select).unwrap_or_default(),
        };
        self.stream_exterior_block(block)?
            .cells
            .into_iter()
            .map(|(cell, loaded)| {
                let (colliders, _) = self.location_colliders(cell.id, &loaded)?;
                Ok(PreparedCell {
                    id: cell.id,
                    loaded,
                    colliders,
                })
            })
            .collect()
    }

    fn location_colliders(
        &self,
        cell: GlobalFormId,
        loaded: &LoadedCell,
    ) -> Result<(Vec<PlacedCollider>, CollisionStats), DataSourceError> {
        let (mut placed, stats) = self.placed_cell_colliders(cell)?;
        for surface in &loaded.surfaces {
            if self
                .order
                .winner(surface.reference)
                .is_none_or(|r| r.tag != tag::LAND)
            {
                continue;
            }
            let arrays = &surface.arrays;
            placed.push(PlacedCollider {
                reference: surface.reference,
                collider: Collider {
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
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|t| [t[0] as u32, t[2] as u32, t[1] as u32])
                            .collect(),
                    },
                },
            });
        }
        Ok((placed, stats))
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Collision for a streamed cell.
//!
//! The renderer and the simulation must agree about where the walls are, so
//! this walks the same references [`crate::data_source::DataSource`] draws,
//! reads the Havok collision each mesh carries through [`soe_physics`], and
//! places it with the same reference transform the visual surfaces get. A
//! triangle a player walks into is therefore the one the artist attached to
//! the model, not a hull derived from the render geometry.
//!
//! The world this builds is [`soe_physics::SimpleWorld`], plain Rust with no
//! native library and no Godot. That is on purpose and `docs/physics.md`
//! carries the reasoning: the authoritative simulation stays in our process
//! so the per tick state hash can see it and a headless run can reproduce it.

use crate::data_source::{placement_iso, DataSource, DataSourceError};
use soe_esm::records::refr::Refr;
use soe_esm::records::stat::Static;
use soe_esm::{tag, GlobalFormId};
use soe_physics::{Collider, PhysicsWorld, SimpleWorld};

/// What building a cell's collision found. Counts are per reference, not per
/// base model, so a mesh placed twenty times counts twenty times, which is
/// what the world actually holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollisionStats {
    /// Placed `STAT` references with a model, the same set the renderer draws.
    pub references: usize,
    /// References whose model carried at least one collider.
    pub with_collision: usize,
    /// Colliders read, before the static filter.
    pub colliders: usize,
    /// Bodies added to the world, so colliders that passed the static filter.
    pub bodies: usize,
    /// World space primitives those bodies flattened to.
    pub primitives: usize,
    /// Models that failed to read or parse. They are skipped, not fatal.
    pub failed: usize,
    /// Collision blocks the shape layer could not decode, by reason, summed
    /// over every reference. Vanilla carries a few of these, see
    /// `docs/physics.md`.
    pub skipped_blocks: usize,
}

/// A cell's static collision, ready to step a character through.
pub struct CellCollision {
    pub world: SimpleWorld,
    pub stats: CollisionStats,
}

impl DataSource {
    /// Builds the static collision world for one cell.
    ///
    /// Sequential on purpose: a cell is a few hundred models and the shape
    /// layer is cheap next to texture decoding, so there is nothing here worth
    /// the threading the visual path needs. Measure before changing that.
    ///
    /// # Errors
    ///
    /// Only for a failure that stops the walk. A model that cannot be read or
    /// parsed is counted in [`CollisionStats::failed`] and skipped, the same
    /// way the visual path treats one.
    pub fn collide_cell_id(&self, cell_id: GlobalFormId) -> Result<CellCollision, DataSourceError> {
        let (colliders, stats) = self.cell_colliders(cell_id)?;
        let mut world = SimpleWorld::new();
        let mut stats = stats;
        for col in colliders.iter().filter(|c| c.is_static()) {
            let before = world.primitive_count();
            world.add_static(col);
            stats.primitives += world.primitive_count() - before;
            stats.bodies += 1;
        }
        Ok(CellCollision { world, stats })
    }

    /// Every collider a cell's placed statics carry, in world space.
    ///
    /// The two worlds start here. [`Self::collide_cell_id`] feeds these to the
    /// headless [`SimpleWorld`], and under the `godot` feature
    /// [`crate::godot_physics::add_statics`] feeds the same list to Godot's
    /// physics server, so the simulation and the renderer cannot disagree
    /// about where a wall is.
    ///
    /// # Errors
    ///
    /// Only for a failure that stops the walk. A model that cannot be read is
    /// counted in [`CollisionStats::failed`] and skipped.
    pub fn cell_colliders(
        &self,
        cell_id: GlobalFormId,
    ) -> Result<(Vec<Collider>, CollisionStats), DataSourceError> {
        let _span = soe_trace::Span::new("host.collide_cell", "host");
        let mut stats = CollisionStats::default();
        let mut out: Vec<Collider> = Vec::new();

        for id in self.cell_index().references(cell_id).iter().copied() {
            let Some(rref) = self.order().winner_ref(id) else {
                continue;
            };
            let rec = self.order().record(rref);
            if rec.tag != tag::REFR {
                continue;
            }
            let Ok(refr) = Refr::from_record(rec) else {
                continue;
            };
            let base_id = self.order().resolve(rref.file, refr.base);
            let Some(base_rec) = self.order().winner(base_id) else {
                continue;
            };
            if base_rec.tag != tag::STAT {
                continue;
            }
            let Ok(stat) = Static::from_record(base_rec) else {
                continue;
            };
            let Some(model) = stat.model.filter(|m| !m.is_empty()) else {
                continue;
            };
            stats.references += 1;
            match self.colliders_of(&model, &refr) {
                Ok(collected) => {
                    stats.skipped_blocks += collected.skipped;
                    if collected.colliders.is_empty() {
                        continue;
                    }
                    stats.with_collision += 1;
                    stats.colliders += collected.colliders.len();
                    out.extend(collected.colliders);
                }
                Err(e) => {
                    eprintln!("collide_cell: {model}: {e}");
                    stats.failed += 1;
                }
            }
        }
        Ok((out, stats))
    }

    /// One reference's colliders, in world space.
    fn colliders_of(&self, model: &str, refr: &Refr) -> Result<RefColliders, DataSourceError> {
        let _span = soe_trace::Span::new("host.collider", "host");
        let bytes = self.vfs().read(&format!("meshes/{model}"))?;
        let file = soe_nif::parse(&bytes)?;
        let collected =
            soe_physics::collect(&file).map_err(|e| DataSourceError::Collision(e.to_string()))?;
        let placement = placement_iso(refr);
        let colliders = collected
            .colliders
            .into_iter()
            .map(|mut c| {
                c.transform = placement.then(&c.transform);
                c
            })
            .collect();
        Ok(RefColliders {
            colliders,
            skipped: collected.skipped.len(),
        })
    }
}

struct RefColliders {
    colliders: Vec<Collider>,
    skipped: usize,
}

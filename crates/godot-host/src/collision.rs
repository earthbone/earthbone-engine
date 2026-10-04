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

use std::collections::HashMap;

use crate::data_source::{model_for, placement_iso, DataSource, DataSourceError};
use soe_esm::records::refr::Refr;
use soe_esm::{tag, GlobalFormId};
use soe_physics::{BodyId, Collider, Iso, PhysicsWorld, Shape, SimpleWorld};

#[cfg(test)]
mod furniture_tests;
#[cfg(test)]
mod scene_tests;

/// What building a cell's collision found. Counts are per reference, not per
/// base model, so a mesh placed twenty times counts twenty times, which is
/// what the world actually holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollisionStats {
    /// Placed statics, furniture, containers and doors selected by the renderer.
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
    /// Prepared static shapes shared by the controller and host query space.
    pub colliders: Vec<Collider>,
    pub world: SimpleWorld,
    pub stats: CollisionStats,
    /// Placed references owning each body. Several bodies may belong to one reference.
    pub references: HashMap<BodyId, GlobalFormId>,
    /// Selected placed colliders, retaining reference ids for host queries.
    pub placed: Vec<PlacedCollider>,
}

/// One placed collision shape with the reference identity used by activation.
pub struct PlacedCollider {
    pub reference: GlobalFormId,
    pub collider: Collider,
    /// The reference's placement at load time, in Skyrim space. Lets a
    /// runtime move be expressed as a delta against this baseline rather
    /// than against `collider.transform`, which already has it baked in.
    pub original: Iso,
}

impl DataSource {
    /// Builds static collision and maps each body back to its placed reference.
    ///
    /// # Errors
    ///
    /// Only for a failure that stops the walk. Unreadable models are counted
    /// in [CollisionStats::failed] and skipped.
    pub fn collide_cell_id(&self, cell_id: GlobalFormId) -> Result<CellCollision, DataSourceError> {
        let (colliders, mut stats) = self.placed_cell_colliders(cell_id)?;
        let mut world = SimpleWorld::new();
        let mut references = HashMap::new();
        for placed in colliders.iter().filter(|p| p.collider.is_static()) {
            let before = world.primitive_count();
            let body = world.add_static(&placed.collider);
            references.insert(body, placed.reference);
            stats.primitives += world.primitive_count() - before;
            stats.bodies += 1;
        }
        let placed = colliders
            .iter()
            .filter(|placed| placed.collider.is_static())
            .map(|placed| PlacedCollider {
                reference: placed.reference,
                collider: placed.collider.clone(),
                original: placed.original,
            })
            .collect();
        Ok(CellCollision {
            colliders: colliders
                .into_iter()
                .filter(|placed| placed.collider.is_static())
                .map(|placed| placed.collider)
                .collect(),
            world,
            stats,
            references,
            placed,
        })
    }

    /// Collision shapes without reference metadata for existing physics consumers.
    ///
    /// # Errors
    ///
    /// Propagates failures from [Self::placed_cell_colliders].
    pub fn cell_colliders(
        &self,
        cell_id: GlobalFormId,
    ) -> Result<(Vec<Collider>, CollisionStats), DataSourceError> {
        let (placed, stats) = self.placed_cell_colliders(cell_id)?;
        Ok((placed.into_iter().map(|p| p.collider).collect(), stats))
    }

    /// Every selected collider in world space, retaining its placed reference.
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
    pub fn placed_cell_colliders(
        &self,
        cell_id: GlobalFormId,
    ) -> Result<(Vec<PlacedCollider>, CollisionStats), DataSourceError> {
        let _span = soe_trace::Span::new("host.collide_cell", "host");
        let mut stats = CollisionStats::default();
        let mut out: Vec<PlacedCollider> = Vec::new();

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
            let Some(model) = model_for(base_rec) else {
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
                    let original = collected.placement;
                    out.extend(
                        collected
                            .colliders
                            .into_iter()
                            .map(|collider| PlacedCollider {
                                reference: id,
                                collider,
                                original,
                            }),
                    );
                }
                Err(e) => {
                    soe_trace::warn!(target: "host::collision", system = "host::collision", "collide_cell: {model}: {e}");
                    stats.failed += 1;
                }
            }
        }
        Ok((out, stats))
    }

    /// The controller capsule for an actor base, local to its skeleton's
    /// root: the skeleton's largest authored `CHAR_CONTROLLER` phantom when
    /// it has one, or a generated capsule sized by `scale` otherwise. See
    /// `docs/physics.md`, "Runtime-generated actor controller geometry,
    /// CMB-33", for what "authored" means here and its known gaps.
    ///
    /// This resolves geometry only. CMB-28 (not yet done) installs the
    /// result into the Godot physics server and keeps it synchronized with
    /// the actor's live transform. This method has no caller in the
    /// streamed-cell path yet.
    ///
    /// # Errors
    /// Propagates a missing or unparsable skeleton mesh.
    pub fn actor_controller_shape(
        &self,
        skeleton_path: &str,
        scale: f32,
    ) -> Result<(Iso, Shape), DataSourceError> {
        let bytes = self.vfs().read(skeleton_path)?;
        let file = soe_nif::parse(&bytes)
            .map_err(|e| DataSourceError::Collision(format!("{skeleton_path}: {e}")))?;
        let collected = soe_physics::collect(&file)
            .map_err(|e| DataSourceError::Collision(format!("{skeleton_path}: {e}")))?;
        Ok(soe_physics::controller_shape(&collected, scale))
    }

    /// One reference's colliders, in world space.
    fn colliders_of(&self, model: &str, refr: &Refr) -> Result<RefColliders, DataSourceError> {
        let _span = soe_trace::Span::new("host.collider", "host");
        let bytes = self.vfs().read(&format!("meshes/{model}"))?;
        let file = crate::data_source::placed_model(&bytes)?;
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
            placement,
        })
    }
}

struct RefColliders {
    colliders: Vec<Collider>,
    skipped: usize,
    placement: Iso,
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Resident static collision follows World enable and door state.
//! Sources are UESP REFR XESP and CK SetOpen, with the HST-21 doorway check.
//! Cached booleans record applied host state, rather than owning game state.

use std::collections::{BTreeMap, BTreeSet};

use godot::prelude::*;
use soe_esm::tag;
use soe_physics::PhysicsWorld;

use super::{BodyId, GlobalFormId, PlayerNode};
use std::collections::HashMap;

/// Main-thread time one frame may spend on watching newly resident references.
/// Applying enable-state changes may use the same amount again. The backlog
/// carries the rest to the next frame.
const SYNC_BUDGET: std::time::Duration = std::time::Duration::from_micros(12);
/// References moved into or out of the resident set between two clock reads.
const EXPAND_SLICE: usize = 64;

/// Records a static body for a reference in both lookup directions.
pub(super) fn track_collision_body(
    references: &mut HashMap<BodyId, GlobalFormId>,
    bodies: &mut HashMap<GlobalFormId, Vec<BodyId>>,
    body: BodyId,
    id: GlobalFormId,
) {
    references.insert(body, id);
    bodies.entry(id).or_default().push(body);
}

/// Forgets a static body that left with its cell.
pub(super) fn untrack_collision_body(
    references: &mut HashMap<BodyId, GlobalFormId>,
    bodies: &mut HashMap<GlobalFormId, Vec<BodyId>>,
    body: BodyId,
) {
    if let Some(id) = references.remove(&body) {
        if let Some(list) = bodies.get_mut(&id) {
            list.retain(|candidate| *candidate != body);
            if list.is_empty() {
                bodies.remove(&id);
            }
        }
    }
}

/// The per-reference collision tracking a departing cell clears.
type ResidentState<'a> = (
    &'a mut BTreeMap<GlobalFormId, GlobalFormId>,
    &'a mut BTreeMap<GlobalFormId, bool>,
    &'a mut BTreeSet<GlobalFormId>,
    &'a mut soe_game::world::ReferenceChanges,
);

/// Drops the given references of a departed cell from collision tracking.
fn drop_residents(references: &[GlobalFormId], cell: GlobalFormId, state: ResidentState<'_>) {
    let (residents, visibility, backlog, changes) = state;
    for id in references {
        if residents.get(id) == Some(&cell) {
            residents.remove(id);
        }
        visibility.remove(id);
        backlog.remove(id);
        changes.unwatch(*id);
    }
}

impl PlayerNode {
    /// Rebuilds the reference to bodies lookup after `collision_references` was
    /// replaced, at the call that replaced it and not inside the timed sync.
    pub(super) fn rebuild_collision_bodies(&mut self) {
        self.collision_bodies.clear();
        for (&body, &id) in &self.collision_references {
            self.collision_bodies.entry(id).or_default().push(body);
        }
    }

    pub(super) fn sync_reference_collision(&mut self) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Render);
        let _phase = crate::scene_timing::Scope::new(soe_trace::System::Physics);
        let Some(space) = self.base().get_world_3d().map(|world| world.get_space()) else {
            return;
        };
        let (Some(source), Some(game), Some(physics)) =
            (&self.data_source, &self.game, &mut self.world)
        else {
            return;
        };
        let mut cells: BTreeSet<_> = self.exterior.as_ref().map_or_else(
            || self.loaded_cell.into_iter().collect(),
            |exterior| exterior.cells.keys().copied().collect(),
        );
        if let Some(world) = self
            .loaded_cell
            .and_then(|cell| source.exterior_world(cell))
        {
            cells.extend(source.cell_index().world_persistent_cell(world));
        }
        let world = game.world();
        let started = std::time::Instant::now();
        let reset = std::mem::take(&mut self.collision_reset);
        let budget = SYNC_BUDGET;
        if reset {
            self.collision_cells.clear();
            self.collision_residents.clear();
            self.collision_visibility.clear();
            self.collision_pending_cells.clear();
            self.collision_removing.clear();
            self.collision_expand_offset = 0;
            self.collision_pending_watch.clear();
            self.collision_backlog.clear();
            self.collision_changes = soe_game::world::ReferenceChanges::default();
        }
        if cells != self.collision_cells {
            let left: Vec<_> = self.collision_cells.difference(&cells).copied().collect();
            let entered: Vec<_> = cells.difference(&self.collision_cells).copied().collect();
            let front = self.collision_pending_cells.front().copied();
            self.collision_pending_cells
                .retain(|cell| cells.contains(cell));
            if self.collision_pending_cells.front().copied() != front {
                self.collision_expand_offset = 0;
            }
            for cell in left {
                self.collision_removing.push_back((cell, 0));
            }
            for cell in entered {
                // CONTEXT: A cell that left and came back before its removal ran
                // would lose the residents it just regained, so that removal
                // finishes first. The case is rare.
                while let Some(index) = self
                    .collision_removing
                    .iter()
                    .position(|(candidate, _)| *candidate == cell)
                {
                    if let Some((cell, offset)) = self.collision_removing.remove(index) {
                        let references = source.cell_index().references(cell);
                        drop_residents(
                            &references[offset.min(references.len())..],
                            cell,
                            (
                                &mut self.collision_residents,
                                &mut self.collision_visibility,
                                &mut self.collision_backlog,
                                &mut self.collision_changes,
                            ),
                        );
                    }
                }
                self.collision_pending_cells.push_back(cell);
            }
            self.collision_cells = cells;
        }
        // CONTEXT: While walking, a cell leaving the resident set is dropped, and
        // a cell entering it is expanded and its references watched, in slices
        // that stay inside the frame budget, so a streaming frame costs about
        // what an idle one does. Those references stay unwatched for a few frames,
        // and their enable state applies once they are watched.
        while let Some(&(cell, offset)) = self.collision_removing.front() {
            if started.elapsed() >= budget {
                break;
            }
            let references = source.cell_index().references(cell);
            let from = offset.min(references.len());
            let end = (from + EXPAND_SLICE).min(references.len());
            drop_residents(
                &references[from..end],
                cell,
                (
                    &mut self.collision_residents,
                    &mut self.collision_visibility,
                    &mut self.collision_backlog,
                    &mut self.collision_changes,
                ),
            );
            if end >= references.len() {
                self.collision_removing.pop_front();
            } else if let Some(entry) = self.collision_removing.front_mut() {
                entry.1 = end;
            }
        }
        while let Some(&cell) = self.collision_pending_cells.front() {
            if started.elapsed() >= budget {
                break;
            }
            let references = source.cell_index().references(cell);
            let end = (self.collision_expand_offset + EXPAND_SLICE).min(references.len());
            for id in &references[self.collision_expand_offset.min(end)..end] {
                self.collision_residents.insert(*id, cell);
                self.collision_pending_watch.push_back(*id);
            }
            if end >= references.len() {
                self.collision_pending_cells.pop_front();
                self.collision_expand_offset = 0;
            } else {
                self.collision_expand_offset = end;
            }
        }
        // CONTEXT: The streamer built each cell's bodies from authored data, so a
        // newly watched reference only needs work when it is disabled and still
        // holds bodies. Watching resolves its enable state, which is the cost
        // this loop spreads over frames.
        while started.elapsed() < budget {
            let Some(id) = self.collision_pending_watch.pop_front() else {
                break;
            };
            if !self.collision_residents.contains_key(&id) {
                continue;
            }
            if self.collision_changes.watch_quiet(&world, id) == Some(false)
                && self.collision_bodies.contains_key(&id)
            {
                self.collision_backlog.insert(id);
            }
        }
        self.collision_backlog
            .extend(self.collision_changes.changed_within(&world, SYNC_BUDGET));
        let mut handled = false;
        while !handled || started.elapsed() < budget + SYNC_BUDGET {
            let Some(id) = self.collision_backlog.pop_first() else {
                break;
            };
            handled = true;
            let Some(&cell) = self.collision_residents.get(&id) else {
                continue;
            };
            if !self.collision_cells.contains(&cell) {
                continue;
            }
            if source
                .order()
                .winner(id)
                .is_none_or(|record| record.tag != tag::REFR)
            {
                continue;
            }
            let visible =
                world.reference_enabled(id) && !crate::godot_physics::is_opened_door(&world, id);
            let applied = self
                .collision_visibility
                .get(&id)
                .copied()
                // CONTEXT: The streamer builds bodies for every reference with a
                // static collider, enabled or not. With no recorded state, a
                // reference holding bodies is applied, and one without bodies
                // has nothing to build or remove, so it counts as unchanged.
                .unwrap_or_else(|| visible || self.collision_bodies.contains_key(&id));
            self.collision_visibility.insert(id, visible);
            if applied == visible {
                continue;
            }
            let bodies = self.collision_bodies.remove(&id).unwrap_or_default();
            for body in bodies {
                physics.remove(body);
                self.collision_references.remove(&body);
                if let Some(exterior) = &mut self.exterior {
                    for bodies in exterior.cells.values_mut() {
                        bodies.retain(|candidate| *candidate != body);
                    }
                }
            }
            self.query_collision.remove_reference(id);
            if !visible {
                continue;
            }
            let Ok(mut placed) = source.placed_reference_colliders(id) else {
                continue;
            };
            placed.retain(|placed| placed.collider.is_static());
            if let Some(pose) = world.ref_placement(id) {
                let target = soe_physics::Iso::placement(pose.position, pose.rotation, pose.scale);
                for placed in &mut placed {
                    let delta = target.then(&placed.original.inverse());
                    placed.collider.transform = delta.then(&placed.collider.transform);
                    placed.original = target;
                }
            }
            for placed in &placed {
                let body = physics.add_static(&placed.collider);
                track_collision_body(
                    &mut self.collision_references,
                    &mut self.collision_bodies,
                    body,
                    id,
                );
                if let Some(exterior) = &mut self.exterior {
                    if let Some(bodies) = exterior.cells.get_mut(&cell) {
                        bodies.push(body);
                    }
                }
            }
            if !placed.is_empty() {
                self.query_collision
                    .append(crate::godot_physics::add_placed_query_statics(
                        space, &placed,
                    ));
            }
        }
        self.query_collision.sync_placements(&world, space);
        self.query_collision
            .set_collisions_enabled(world.debug_collisions_enabled);
        self.query_collision.sync_collision_state();
    }
}

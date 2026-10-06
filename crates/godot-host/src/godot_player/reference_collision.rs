// SPDX-License-Identifier: GPL-3.0-or-later

//! Resident static collision follows World enable state, using UESP REFR XESP.
//! Cached booleans record applied host state, rather than owning game state.

use std::collections::{BTreeMap, BTreeSet};

use godot::prelude::*;
use soe_esm::{tag, GlobalFormId};
use soe_physics::PhysicsWorld;

use super::PlayerNode;

impl PlayerNode {
    pub(super) fn sync_reference_collision(&mut self) {
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
        let references: BTreeMap<GlobalFormId, GlobalFormId> = cells
            .iter()
            .flat_map(|cell| {
                source
                    .cell_index()
                    .references(*cell)
                    .iter()
                    .map(|id| (*id, *cell))
            })
            .collect();
        self.collision_visibility
            .retain(|id, _| references.contains_key(id));
        let world = game.world();
        for (id, cell) in references {
            if source
                .order()
                .winner(id)
                .is_none_or(|record| record.tag != tag::REFR)
            {
                continue;
            }
            let visible = world.reference_enabled(id);
            let applied = self
                .collision_visibility
                .get(&id)
                .copied()
                .unwrap_or_else(|| {
                    self.collision_references
                        .values()
                        .any(|reference| *reference == id)
                });
            self.collision_visibility.insert(id, visible);
            if applied == visible {
                continue;
            }
            let bodies: Vec<_> = self
                .collision_references
                .iter()
                .filter(|(_, reference)| **reference == id)
                .map(|(body, _)| *body)
                .collect();
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
                self.collision_references.insert(body, id);
                if let Some(exterior) = &mut self.exterior {
                    if let Some(bodies) = exterior.cells.get_mut(&cell) {
                        bodies.push(body);
                    }
                }
            }
            self.query_collision
                .append(crate::godot_physics::add_placed_query_statics(
                    space, &placed,
                ));
        }
        self.query_collision.sync_placements(&world, space);
        self.query_collision
            .set_collisions_enabled(world.debug_collisions_enabled);
        self.query_collision.sync_collision_state();
    }
}

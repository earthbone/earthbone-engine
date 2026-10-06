// SPDX-License-Identifier: GPL-3.0-or-later

//! What the package driver (`crate::ai`) reads and calls on the world: the
//! loaded actors and quests, the player, editor positions, cell flags, and
//! the `EvaluatePackage` and tick entry points.

use soe_esm::records::refr::Refr;
use soe_esm::GlobalFormId;

use super::{le_f32, Alloc, World, T};

impl World {
    /// Advance actor-owned process clocks without repeated actor-table lookups.
    pub(crate) fn schedule_actor_updates(&mut self, dt: f64) -> crate::ai::process::Schedule {
        use crate::ai::process::{coordinate_space, level_from_state, PlayerContext, Schedule};
        use crate::ai::ProcessClock;
        use std::collections::HashMap;

        let player = self
            .player
            .filter(|id| self.actors.contains_key(id))
            .and_then(|id| self.refs.get(&id))
            .map(|reference| PlayerContext {
                position: reference.position,
                cell: reference.cell,
                space: coordinate_space(self, reference.cell),
            });
        let mut spaces = HashMap::new();
        if let Some(player) = player.as_ref().filter(|player| player.space.is_some()) {
            for actor in self.actors.keys() {
                if let Some(cell) = self.refs.get(actor).and_then(|reference| {
                    (reference.cell != player.cell
                        && crate::ai::distance(player.position, reference.position) <= 16384.0)
                        .then_some(reference.cell)
                        .flatten()
                }) {
                    spaces
                        .entry(cell)
                        .or_insert_with(|| coordinate_space(self, Some(cell)));
                }
            }
        }
        let mut schedule = Schedule {
            actors: Vec::with_capacity(self.actors.len()),
            updates: Vec::new(),
            inactive: Vec::new(),
            counts: [0; 4],
            update_counts: [0; 4],
        };
        for (&actor, state) in &mut self.actors {
            schedule.actors.push(actor);
            let level = level_from_state(
                state,
                self.refs.get(&actor),
                self.player == Some(actor),
                player.as_ref(),
                &spaces,
            );
            schedule.counts[level.index()] += 1;
            if state.dead || state.unconscious {
                schedule.inactive.push(actor);
            }
            if let Some(elapsed) = ProcessClock::advance(
                &mut state.process_clock,
                actor,
                level,
                dt,
                state.ai.reevaluate,
            ) {
                schedule.update_counts[level.index()] += 1;
                schedule.updates.push((actor, elapsed));
            }
        }
        schedule.actors.sort_unstable();
        schedule.inactive.sort_unstable();
        schedule.updates.sort_unstable_by_key(|(actor, _)| *actor);
        schedule
    }

    /// Mutate an actor already selected from the loaded actor table.
    pub(crate) fn loaded_actor_mut(&mut self, id: GlobalFormId) -> Option<&mut super::ActorState> {
        self.actors.get_mut(&id)
    }

    /// Every actor whose run-time state has been built.
    pub fn loaded_actors(&self) -> Vec<GlobalFormId> {
        let mut out: Vec<GlobalFormId> = self.actors.keys().copied().collect();
        out.sort();
        out
    }

    /// Every quest whose run-time state has been built.
    pub fn loaded_quests(&self) -> Vec<GlobalFormId> {
        let mut out: Vec<GlobalFormId> = self.quests.keys().copied().collect();
        out.sort();
        out
    }

    /// Live alias packages in quest priority and FormID order.
    pub(crate) fn actor_alias_packages(&self, actor: GlobalFormId) -> Vec<crate::ai::Candidate> {
        let mut packages = Vec::new();
        for (&quest, state) in &self.quests {
            if !state.running || !state.aliases.values().any(|filled| *filled == Some(actor)) {
                continue;
            }
            for alias in &state.def.aliases {
                if alias.packages.is_empty()
                    || state.aliases.get(&alias.id).copied().flatten() != Some(actor)
                {
                    continue;
                }
                for raw in &alias.packages {
                    if let Some(package) = self.resolve_from(quest, *raw) {
                        packages.push((
                            std::cmp::Reverse(state.def.priority),
                            quest,
                            crate::ai::Candidate {
                                package,
                                alias: Some((quest, alias.id)),
                            },
                        ));
                    }
                }
            }
        }
        packages.sort_by_key(|(priority, quest, _)| (*priority, *quest));
        packages
            .into_iter()
            .map(|(_, _, package)| package)
            .collect()
    }

    pub fn is_player(&self, id: GlobalFormId) -> bool {
        self.player == Some(id)
    }

    /// The position the reference was placed at in its record.
    pub fn editor_position(&self, id: GlobalFormId) -> Option<[f32; 3]> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        match rec.tag.as_bytes() {
            b"REFR" => Refr::from_record(rec).ok()?.transform.map(|t| t.position),
            b"ACHR" => {
                let d = rec.find(T(b"DATA")).filter(|d| d.len() >= 12)?;
                Some([le_f32(d), le_f32(&d[4..]), le_f32(&d[8..])])
            }
            _ => None,
        }
    }

    /// The interior flag of a `CELL` record's `DATA`.
    pub fn cell_is_interior(&self, cell: GlobalFormId) -> bool {
        self.record(cell)
            .filter(|r| r.tag == T(b"CELL"))
            .and_then(|r| r.find(T(b"DATA")))
            .and_then(|d| d.first())
            .is_some_and(|f| f & 1 != 0)
    }

    /// Whether the exterior cell belongs to a worldspace with max-height data.
    pub fn cell_has_worldspace_max_height_data(&self, cell: GlobalFormId) -> bool {
        let Some(cell_ref) = self.order.winner_ref(cell) else {
            return false;
        };
        let cell_record = self.order.record(cell_ref);
        if cell_record.tag != T(b"CELL") {
            return false;
        }
        let Some(raw_world) = cell_record.placement.world else {
            return false;
        };
        let worldspace = self.order.resolve(cell_ref.file, raw_world);
        self.record(worldspace)
            .filter(|record| record.tag == T(b"WRLD"))
            .is_some_and(|record| record.find(T(b"MHDT")).is_some())
    }

    /// Build the run-time state of an actor so the driver ticks it.
    pub fn drive_actor(&mut self, id: GlobalFormId) -> bool {
        self.actor(id).is_some()
    }

    /// `Actor.EvaluatePackage`: re-evaluate the stack now.
    pub fn evaluate_package(
        &mut self,
        actor: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Option<GlobalFormId> {
        crate::ai::evaluate(self, actor, alloc)
    }

    /// `Actor.GetCurrentPackage`.
    pub fn current_package(&self, actor: GlobalFormId) -> Option<GlobalFormId> {
        self.actors.get(&actor).and_then(|a| a.ai.package)
    }

    /// `Actor.MoveToPackageLocation`: pop an actor to its selected package's
    /// location center.
    ///
    /// # Returns
    ///
    /// Returns `false` when the actor has no selected package location, the
    /// location is non-finite or the actor lacks a live reference.
    pub fn move_to_package_location(&mut self, actor: GlobalFormId) -> bool {
        let Some((position, cell)) = self
            .actor(actor)
            .map(|state| (state.ai.center, state.ai.center_cell))
        else {
            return false;
        };
        let Some(position) = position.filter(|point| point.iter().all(|v| v.is_finite())) else {
            return false;
        };
        let Some(reference) = self.reference(actor) else {
            return false;
        };
        reference.position = position;
        reference.cell = cell;
        true
    }

    /// Advance every driven actor by `real_seconds`.
    pub fn ai_tick(&mut self, real_seconds: f64, alloc: Alloc<'_>) {
        crate::ai::tick(self, real_seconds, alloc);
    }
}

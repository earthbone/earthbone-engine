// SPDX-License-Identifier: GPL-3.0-or-later

//! Initial exterior actor placement from xEdit ACHR DATA, REFR XTEL and PACK locations.
//! Attachment selects records and package positions, without preparing host models.

use super::World;
use soe_esm::{records::refr::Refr, tag, GlobalFormId};
use std::collections::BTreeSet;

impl World {
    /// Place first-time exterior actors at explicit destinations of the current package.
    /// Running packages and previously moved references retain their live poses.
    pub fn initialize_exterior_actor_placements(&mut self, worldspace: GlobalFormId) {
        let index = self.order().build_cell_index();
        let mut cells: BTreeSet<_> = self
            .attached_cell_ids()
            .into_iter()
            .filter(|cell| self.cell_coordinate_space(*cell) == worldspace)
            .collect();
        let grids: BTreeSet<_> = cells
            .iter()
            .filter_map(|cell| index.grid_of(*cell).map(|grid| (grid.x, grid.y)))
            .collect();
        cells.extend(index.world_persistent_cell(worldspace));
        let mut interiors = BTreeSet::new();
        for cell in &cells {
            for &id in index.references(*cell) {
                let Some(owner) = self.order().winner_ref(id) else {
                    continue;
                };
                let record = self.order().record(owner);
                if record.tag != tag::REFR {
                    continue;
                }
                let Some(position) = self
                    .editor_position(id)
                    .filter(|position| position.iter().all(|value| value.is_finite()))
                else {
                    continue;
                };
                let grid = (
                    (position[0] / 4096.0).floor() as i32,
                    (position[1] / 4096.0).floor() as i32,
                );
                if !grids.contains(&grid) {
                    continue;
                }
                let Ok(reference) = Refr::from_record(record) else {
                    continue;
                };
                let Some(teleport) = reference.teleport else {
                    continue;
                };
                let door = self.order().resolve(owner.file, teleport.door);
                if let Some(cell) = self
                    .order()
                    .winner_ref(door)
                    .and_then(|owner| {
                        self.order()
                            .record(owner)
                            .placement
                            .cell
                            .map(|raw| self.order().resolve(owner.file, raw))
                    })
                    .filter(|cell| index.is_interior(*cell))
                {
                    interiors.insert(cell);
                }
            }
        }
        cells.extend(interiors);
        let candidates: BTreeSet<_> = cells
            .iter()
            .flat_map(|cell| index.references(*cell).iter().copied())
            .filter(|id| {
                self.order()
                    .winner(*id)
                    .is_some_and(|record| record.tag == tag::ACHR)
            })
            .collect();
        for id in candidates {
            if !self.reference_enabled(id)
                || self.actor_ref(id).is_some_and(|state| {
                    state.ai.package.is_some() || state.ai.last_evaluation != 0.0
                })
            {
                continue;
            }
            let Some(authored) = self.editor_position(id) else {
                continue;
            };
            let Some(reference) = self.reference(id) else {
                continue;
            };
            if reference.position != authored {
                continue;
            }
            let Some(candidate) = crate::ai::select(self, id) else {
                continue;
            };
            let (_, location, _) = crate::ai::procedure_of(self, candidate.package);
            let mut ai = self
                .actor(id)
                .map(|actor| actor.ai.clone())
                .unwrap_or_default();
            ai.start_position = authored;
            let alias = candidate.alias.or_else(|| {
                let raw = crate::ai::package(self, candidate.package)?.owner_quest?;
                Some((self.resolve_from(candidate.package, raw)?, 0))
            });
            let Some(location) = location.and_then(|location| {
                crate::ai::resolve_location(self, id, candidate.package, alias, &ai, &location)
            }) else {
                continue;
            };
            if !location.position.iter().all(|value| value.is_finite())
                || location
                    .cell
                    .is_none_or(|cell| self.cell_coordinate_space(cell) != worldspace)
            {
                continue;
            }
            if let Some(reference) = self.reference(id) {
                reference.position = location.position;
                reference.cell = location.cell;
            }
        }
    }
}

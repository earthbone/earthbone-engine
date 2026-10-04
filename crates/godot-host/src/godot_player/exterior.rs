// SPDX-License-Identifier: GPL-3.0-or-later

//! Walking grid deltas follow OpenMW Scene::changeCellGrid and the host's
//! hysteresis planner. Render and physics resources retain their CELL owners.

use std::collections::{BTreeMap, HashMap, HashSet};

use soe_esm::GlobalFormId;
use soe_physics::{BodyId, PhysicsWorld};

use crate::streaming::{CellCoord, CellStreamer, StreamBudget, StreamConfig, StreamCost};
use crate::units::SkyrimPos;

use super::PlayerNode;

pub(super) struct ExteriorRuntime {
    pub world: GlobalFormId,
    pub planner: CellStreamer,
    pub cells: BTreeMap<GlobalFormId, Vec<BodyId>>,
}

pub(super) fn config() -> StreamConfig {
    StreamConfig {
        load_radius: 1,
        unload_radius: 2,
        max_loads_per_tick: soe_game::EXTERIOR_CELL_LOADS_PER_UPDATE,
        max_unloads_per_tick: 4,
    }
}

impl PlayerNode {
    pub(super) fn advance_exterior(&mut self) -> bool {
        let feet = self.feet();
        let focus = SkyrimPos::new(feet.x, feet.y, feet.z);
        let (Some(exterior), Some(source), Some(streamer), Some(physics), Some(game)) = (
            self.exterior.as_mut(),
            self.data_source.as_ref(),
            self.game_streamer.as_mut(),
            self.world.as_mut(),
            self.game.as_mut(),
        ) else {
            return true;
        };
        game.record_exterior_cell_loads(0);
        let coord = CellCoord::containing(focus);
        let Some(cell) = source
            .cell_index()
            .exterior_cell(exterior.world, coord.x, coord.y)
        else {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: no exterior cell at {},{}", coord.x, coord.y);
            return false;
        };
        // Commit the planner only after every requested cell has prepared successfully.
        let mut planner = exterior.planner.clone();
        let budget = StreamBudget {
            triangles: usize::try_from(self.exterior_triangle_budget).unwrap_or_default(),
            bytes: usize::try_from(self.exterior_byte_budget).unwrap_or_default(),
        };
        let candidates = if budget == StreamBudget::default() {
            planner.update(focus).load
        } else {
            planner.candidates(focus)
        };
        let prepared = match source.prepare_grid_cells(exterior.world, &candidates) {
            Ok(cells) => cells,
            Err(error) => {
                soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: exterior preparation failed: {error}");
                return false;
            }
        };
        let costs: HashMap<CellCoord, StreamCost> = prepared
            .iter()
            .filter_map(|cell| {
                candidates.iter().find_map(|coord| {
                    (source
                        .cell_index()
                        .exterior_cell(exterior.world, coord.x, coord.y)
                        == Some(cell.id))
                    .then(|| (*coord, cell.stream_cost()))
                })
            })
            .collect();
        planner = exterior.planner.clone();
        let plan = planner.update_with_budget(focus, budget, |coord| {
            costs.get(&coord).copied().unwrap_or_default()
        });
        if plan.is_empty() && self.loaded_cell == Some(cell) {
            return true;
        }
        let selected: HashSet<GlobalFormId> = plan
            .load
            .iter()
            .filter_map(|coord| {
                source
                    .cell_index()
                    .exterior_cell(exterior.world, coord.x, coord.y)
            })
            .collect();
        let prepared: Vec<_> = prepared
            .into_iter()
            .filter(|cell| selected.contains(&cell.id))
            .collect();
        let departed: Vec<_> = plan
            .unload
            .iter()
            .filter_map(|coord| {
                source
                    .cell_index()
                    .exterior_cell(exterior.world, coord.x, coord.y)
            })
            .filter(|id| exterior.cells.contains_key(id))
            .collect();
        if !streamer.is_instance_valid()
            || !streamer
                .bind_mut()
                .apply_cell_delta(&prepared, &departed, &game.world())
        {
            return false;
        }
        let entered: Vec<_> = prepared.iter().map(|cell| cell.id).collect();
        for cell in &prepared {
            let mut bodies = Vec::new();
            for placed in cell.colliders.iter().filter(|p| p.collider.is_static()) {
                let body = physics.add_static(&placed.collider);
                self.collision_references.insert(body, placed.reference);
                bodies.push(body);
            }
            exterior.cells.insert(cell.id, bodies);
        }
        for id in &departed {
            if let Some(bodies) = exterior.cells.remove(id) {
                for body in bodies {
                    physics.remove(body);
                    self.collision_references.remove(&body);
                }
            }
        }
        if let Err(error) = game.update_streaming_cells(cell, &entered, &departed) {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: exterior state failed: {error}");
            return false;
        }
        game.record_exterior_cell_loads(entered.len());
        exterior.planner = planner;
        self.loaded_cell = Some(cell);
        soe_trace::info!(target: "host::exterior", system = "host::exterior",
            "EXTERIOR focus={cell} entered={} departed={} resident={}",
            entered.len(),
            departed.len(),
            exterior.cells.len()
        );
        true
    }
}

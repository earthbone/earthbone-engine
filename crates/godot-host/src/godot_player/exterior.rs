// SPDX-License-Identifier: GPL-3.0-or-later

//! Walking grid deltas follow OpenMW Scene::changeCellGrid and the host's
//! hysteresis planner. Render and physics resources retain their CELL owners.

use std::collections::{BTreeMap, VecDeque};
use std::thread::JoinHandle;

use godot::obj::WithBaseField;
use soe_esm::GlobalFormId;
use soe_physics::{BodyId, PhysicsWorld};

use crate::streaming::{
    CellCoord, CellRequest, CellStreamer, StreamBudget, StreamConfig, StreamCost,
};
use crate::units::SkyrimPos;

use super::reference_collision::{track_collision_body, untrack_collision_body};
use super::PlayerNode;

pub(super) struct ExteriorRuntime {
    pub world: GlobalFormId,
    pub planner: CellStreamer,
    pub cells: BTreeMap<GlobalFormId, Vec<BodyId>>,
    pub lod_started: bool,
    pub pending: Pending,
}

type Outcome = (
    CellRequest,
    Result<
        Option<(
            crate::data_source::PreparedCell,
            crate::godot_layer::upload::Plan,
        )>,
        String,
    >,
);

#[derive(Default)]
pub(super) struct Pending {
    job: Option<JoinHandle<Vec<Outcome>>>,
    requests: Vec<CellRequest>,
    queue: VecDeque<Outcome>,
    upload: Option<Uploading>,
}

struct Uploading {
    request: CellRequest,
    cell: crate::data_source::PreparedCell,
    plan: crate::godot_layer::upload::Plan,
    totals: crate::godot_layer::StreamTotals,
    placed: usize,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(worker) = self.job.take() {
            let _ = worker.join();
        }
    }
}

fn advance_lod(
    exterior: &mut ExteriorRuntime,
    source: &crate::data_source::DataSource,
    streamer: &mut godot::prelude::Gd<crate::godot_layer::CellStreamerNode>,
    focus: CellCoord,
    full_res_radius: i32,
    lod_radius: i32,
    budget_bytes: i64,
    progressive: bool,
) -> bool {
    let mut node = streamer.bind_mut();
    if !exterior.lod_started {
        let Some((_, world_editor_id)) = source.lod_worldspace(exterior.world) else {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: exterior world {} has no editor id for LOD", exterior.world);
            return false;
        };
        if progressive {
            if !node.begin_walking_lod(
                &world_editor_id,
                focus,
                full_res_radius,
                lod_radius,
                budget_bytes,
            ) {
                return false;
            }
            exterior.lod_started = true;
            return true;
        }
        let terrain = node.stream_terrain_lod(
            godot::prelude::GString::from(world_editor_id.as_str()),
            focus.x,
            focus.y,
            full_res_radius,
            lod_radius,
            budget_bytes,
        );
        let objects = node.stream_object_lod(
            godot::prelude::GString::from(world_editor_id.as_str()),
            focus.x,
            focus.y,
            full_res_radius,
            lod_radius,
            budget_bytes,
        );
        let trees = node.stream_tree_lod(
            godot::prelude::GString::from(world_editor_id.as_str()),
            focus.x,
            focus.y,
            full_res_radius,
            lod_radius,
            budget_bytes,
        );
        if terrain < 0 || objects < 0 || trees < 0 {
            node.clear_terrain_lod();
            node.clear_object_lod();
            node.clear_tree_lod();
            return false;
        }
        exterior.lod_started = true;
        return true;
    }

    node.update_terrain_lod(focus.x, focus.y) >= 0
        && node.update_object_lod(focus.x, focus.y) >= 0
        && node.update_tree_lod(focus.x, focus.y) >= 0
}

pub(super) fn config() -> StreamConfig {
    StreamConfig {
        load_radius: 2,
        unload_radius: 2,
        max_loads_per_tick: soe_game::EXTERIOR_CELL_LOADS_PER_UPDATE,
        max_unloads_per_tick: 4,
    }
}

impl PlayerNode {
    fn exterior_eye(&self) -> SkyrimPos {
        let feet = self.feet();
        let height = self.capsule_height();
        let head = self.rig.head(feet, height);
        let target = self.rig.boom_target(feet, height);
        let clear = if self.rig.mode == crate::camera::CameraMode::Third {
            self.world
                .as_ref()
                .and_then(|world| world.raycast(head, target))
                .map(|hit| {
                    ((hit.point.x - head.x).powi(2)
                        + (hit.point.y - head.y).powi(2)
                        + (hit.point.z - head.z).powi(2))
                    .sqrt()
                })
        } else {
            None
        };
        self.rig.eye(feet, height, clear).position
    }

    pub(super) fn advance_exterior(&mut self) -> bool {
        if self.auto_step && self.exterior_triangle_budget == 0 && self.exterior_byte_budget == 0 {
            return self.advance_exterior_async();
        }
        let _system = soe_trace::SystemScope::new(soe_trace::System::Streaming);
        let feet = self.feet();
        let eye = self.exterior_eye();
        let focus = SkyrimPos::new(feet.x, feet.y, feet.z);
        let space = self.base().get_world_3d().map(|world| world.get_space());
        let (Some(exterior), Some(source), Some(streamer), Some(physics), Some(game)) = (
            self.exterior.as_mut(),
            self.data_source.as_ref(),
            self.game_streamer.as_mut(),
            self.world.as_mut(),
            self.game.as_mut(),
        ) else {
            return true;
        };
        let Some(space) = space else { return false };
        game.record_exterior_cell_loads(0);
        let coord = CellCoord::containing(focus);
        if !advance_lod(
            exterior,
            source,
            streamer,
            coord,
            self.exterior_lod_full_res_radius
                .max(config().unload_radius),
            self.exterior_lod_radius,
            self.exterior_lod_byte_budget,
            self.auto_step,
        ) {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: exterior LOD update failed for {} at {},{}", exterior.world, coord.x, coord.y);
            return false;
        }
        let Some(cell) = source
            .cell_index()
            .exterior_cell(exterior.world, coord.x, coord.y)
        else {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: no exterior cell at {},{}", coord.x, coord.y);
            return false;
        };
        if source.is_small_world(exterior.world) {
            if let Err(error) = game.update_streaming_cells(cell, &[], &[]) {
                soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: small-world focus failed: {error}");
                return false;
            }
            self.loaded_cell = Some(cell);
            return true;
        }
        let direction = self.rig.look_direction();
        exterior.planner.set_view([
            f64::from(direction.x),
            f64::from(direction.y),
            f64::from(direction.z),
        ]);
        let budget = StreamBudget {
            triangles: usize::try_from(self.exterior_triangle_budget).unwrap_or_default(),
            bytes: usize::try_from(self.exterior_byte_budget).unwrap_or_default(),
        };
        let candidates = if budget == StreamBudget::default() {
            exterior.planner.preview_loads(focus)
        } else {
            exterior.planner.candidates(focus)
        };
        let mut outcomes = BTreeMap::new();
        match source.prepare_grid_cells(exterior.world, &candidates) {
            Ok(cells) => {
                let mut prepared: BTreeMap<_, _> =
                    cells.into_iter().map(|cell| (cell.id, cell)).collect();
                for coord in candidates {
                    let ready = source
                        .cell_index()
                        .exterior_cell(exterior.world, coord.x, coord.y)
                        .and_then(|id| prepared.remove(&id));
                    outcomes.insert(coord, Ok(ready));
                }
            }
            Err(_) => {
                for coord in candidates {
                    let outcome = source
                        .prepare_grid_cells(exterior.world, &[coord])
                        .map(|cells| cells.into_iter().next());
                    outcomes.insert(coord, outcome);
                }
            }
        }
        let plan = exterior.planner.update_with_budget(focus, budget, |coord| {
            outcomes
                .get(&coord)
                .and_then(|result| result.as_ref().ok())
                .and_then(Option::as_ref)
                .map_or(StreamCost::default(), |cell| cell.stream_cost())
        });
        let mut prepared = Vec::new();
        let mut ready_requests = Vec::new();
        for request in &plan.requests {
            exterior.planner.mark_loading(*request);
            match outcomes.remove(&request.cell) {
                Some(Ok(Some(cell))) => {
                    exterior.planner.mark_completed(*request);
                    ready_requests.push(*request);
                    prepared.push(cell);
                }
                Some(Ok(None)) => {
                    exterior.planner.mark_completed(*request);
                    exterior.planner.mark_drawable(*request);
                }
                Some(Err(error)) => {
                    soe_trace::error!(target: "host::exterior", system = "host::exterior",
                        "PlayerNode: exterior preparation failed at {},{}: {error}", request.cell.x, request.cell.y);
                    exterior.planner.mark_failed(*request);
                }
                None => {
                    exterior.planner.mark_failed(*request);
                }
            }
        }
        if plan.is_empty() && self.loaded_cell == Some(cell) {
            return streamer.bind_mut().update_distant(
                exterior.world,
                [eye.x, eye.y, eye.z],
                &game.world(),
                false,
            );
        }
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
            for request in ready_requests {
                exterior.planner.mark_failed(request);
            }
            for request in &plan.retirements {
                exterior.planner.mark_retirement_failed(*request);
            }
            return true;
        }
        for request in ready_requests {
            exterior.planner.mark_drawable(request);
        }
        for request in &plan.retirements {
            exterior.planner.mark_retired(*request);
        }
        let entered: Vec<_> = prepared.iter().map(|cell| cell.id).collect();
        for cell in &prepared {
            self.query_collision
                .append(crate::godot_physics::add_placed_query_statics(
                    space,
                    &cell.colliders,
                ));
            let mut bodies = Vec::new();
            for placed in cell.colliders.iter().filter(|p| p.collider.is_static()) {
                let body = physics.add_static(&placed.collider);
                track_collision_body(
                    &mut self.collision_references,
                    &mut self.collision_bodies,
                    body,
                    placed.reference,
                );
                bodies.push(body);
            }
            exterior.cells.insert(cell.id, bodies);
        }
        for id in &departed {
            if let Some(bodies) = exterior.cells.remove(id) {
                for body in bodies {
                    if let Some(reference) = self.collision_references.get(&body) {
                        self.query_collision.remove_reference(*reference);
                    }
                    physics.remove(body);
                    untrack_collision_body(
                        &mut self.collision_references,
                        &mut self.collision_bodies,
                        body,
                    );
                }
            }
        }
        if let Err(error) = game.update_streaming_cells(cell, &entered, &departed) {
            soe_trace::error!(target: "host::exterior", system = "host::exterior", "PlayerNode: exterior state failed: {error}");
            return false;
        }
        game.record_exterior_cell_loads(entered.len());
        if !streamer.bind_mut().update_distant(
            exterior.world,
            [eye.x, eye.y, eye.z],
            &game.world(),
            false,
        ) {
            return false;
        }
        let audio_changed = self.loaded_cell != Some(cell);
        self.loaded_cell = Some(cell);
        soe_trace::info!(target: "host::exterior", system = "host::exterior",
            "EXTERIOR focus={cell} entered={} departed={} resident={}",
            entered.len(),
            departed.len(),
            exterior.cells.len()
        );
        if audio_changed {
            self.sync_cell_ambience(cell);
            self.sync_cell_music(cell);
        }
        true
    }

    fn advance_exterior_async(&mut self) -> bool {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Streaming);
        let _span = soe_trace::Span::new("host.exterior_frame", "host");
        let feet = self.feet();
        let focus = SkyrimPos::new(feet.x, feet.y, feet.z);
        let direction = self.rig.look_direction();
        let eye = self.exterior_eye();
        let space = self.base().get_world_3d().map(|world| world.get_space());
        let (Some(exterior), Some(source), Some(streamer), Some(physics), Some(game), Some(space)) = (
            self.exterior.as_mut(),
            self.data_source.as_ref(),
            self.game_streamer.as_mut(),
            self.world.as_mut(),
            self.game.as_mut(),
            space,
        ) else {
            return true;
        };
        let coord = CellCoord::containing(focus);
        if !advance_lod(
            exterior,
            source,
            streamer,
            coord,
            self.exterior_lod_full_res_radius
                .max(config().unload_radius),
            self.exterior_lod_radius,
            self.exterior_lod_byte_budget,
            self.auto_step,
        ) {
            soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior LOD update failed at {},{}", coord.x, coord.y);
            return false;
        }
        let Some(focus_cell) = source
            .cell_index()
            .exterior_cell(exterior.world, coord.x, coord.y)
        else {
            return false;
        };
        if source.is_small_world(exterior.world) {
            if game.update_streaming_cells(focus_cell, &[], &[]).is_err() {
                return false;
            }
            self.loaded_cell = Some(focus_cell);
            return true;
        }
        game.record_exterior_cell_loads(0);
        exterior.planner.set_view([
            f64::from(direction.x),
            f64::from(direction.y),
            f64::from(direction.z),
        ]);
        let busy = exterior.pending.job.is_some()
            || !exterior.pending.queue.is_empty()
            || exterior.pending.upload.is_some();
        let plan = if busy {
            exterior.planner.update_with_budget(
                focus,
                StreamBudget {
                    bytes: 1,
                    triangles: 0,
                },
                |_| StreamCost {
                    bytes: 2,
                    triangles: 0,
                },
            )
        } else {
            exterior.planner.update(focus)
        };
        if !plan.requests.is_empty() {
            for request in &plan.requests {
                exterior.planner.mark_loading(*request);
            }
            let source = std::sync::Arc::clone(source);
            let world = exterior.world;
            let requests = plan.requests.clone();
            let batch = streamer.bind().batch_uploads();
            let disabled: std::collections::BTreeSet<_> = requests
                .iter()
                .filter_map(|request| {
                    source
                        .cell_index()
                        .exterior_cell(world, request.cell.x, request.cell.y)
                })
                .flat_map(|cell| source.presentation_references(cell))
                .filter(|id| !game.world().reference_enabled(**id))
                .copied()
                .collect();
            match std::thread::Builder::new()
                .name("exterior-prepare".into())
                .spawn(move || {
                    let _span = soe_trace::Span::new("host.exterior_prepare", "host");
                    requests
                        .into_iter()
                        .map(|request| {
                            let result = source
                                .prepare_grid_cells(world, &[request.cell])
                                .map(|cells| {
                                    cells.into_iter().next().map(|mut cell| {
                                        cell.loaded.surfaces.retain(|surface| {
                                            !disabled.contains(&surface.reference)
                                        });
                                        let _capture = cell.loaded.stats.profile.enter();
                                        let plan = crate::godot_layer::upload::Plan::new(
                                            Some(&source),
                                            cell.id,
                                            &cell.loaded.surfaces,
                                            batch,
                                        );
                                        (cell, plan)
                                    })
                                })
                                .map_err(|error| error.to_string());
                            (request, result)
                        })
                        .collect()
                }) {
                Ok(worker) => {
                    exterior.pending.requests = plan.requests.clone();
                    exterior.pending.job = Some(worker);
                }
                Err(error) => {
                    soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior worker: {error}");
                    for request in &plan.requests {
                        exterior.planner.mark_failed(*request);
                    }
                }
            }
        }
        if exterior
            .pending
            .job
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            if let Some(worker) = exterior.pending.job.take() {
                match worker.join() {
                    Ok(results) => {
                        exterior.pending.requests.clear();
                        exterior.pending.queue.extend(results);
                    }
                    Err(_) => {
                        let failed = std::mem::take(&mut exterior.pending.requests);
                        for request in failed {
                            exterior.planner.mark_failed(request);
                        }
                    }
                }
            }
        }
        if exterior.pending.upload.is_none() {
            while let Some((request, outcome)) = exterior.pending.queue.pop_front() {
                if exterior.planner.request(request.cell) != Some(request) {
                    continue;
                }
                match outcome {
                    Ok(Some((cell, upload))) => {
                        exterior.planner.mark_completed(request);
                        exterior.pending.upload = Some(Uploading {
                            request,
                            cell,
                            plan: upload,
                            totals: crate::godot_layer::StreamTotals::default(),
                            placed: 0,
                        });
                        break;
                    }
                    Ok(None) => {
                        exterior.planner.mark_completed(request);
                        exterior.planner.mark_drawable(request);
                    }
                    Err(error) => {
                        soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior preparation: {error}");
                        exterior.planner.mark_failed(request);
                    }
                }
            }
        }
        let mut entered = Vec::new();
        if let Some(mut uploading) = exterior.pending.upload.take() {
            let id = uploading.cell.id;
            if exterior.planner.request(uploading.request.cell) == Some(uploading.request) {
                let _capture = uploading.cell.loaded.stats.profile.enter();
                if !streamer.bind_mut().upload_plan_step(
                    id,
                    &mut uploading.plan,
                    &mut uploading.totals,
                    crate::godot_layer::upload::SLICE_BUDGET,
                ) {
                    exterior.planner.mark_failed(uploading.request);
                    return false;
                }
                if uploading.plan.is_empty() && uploading.placed < uploading.cell.colliders.len() {
                    let end = (uploading.placed + 32).min(uploading.cell.colliders.len());
                    self.query_collision
                        .append(crate::godot_physics::add_placed_query_statics(
                            space,
                            &uploading.cell.colliders[uploading.placed..end],
                        ));
                    uploading.placed = end;
                }
                if !uploading.plan.is_empty() || uploading.placed < uploading.cell.colliders.len() {
                    exterior.pending.upload = Some(uploading);
                } else {
                    let mut bodies = Vec::new();
                    for placed in uploading
                        .cell
                        .colliders
                        .iter()
                        .filter(|p| p.collider.is_static())
                    {
                        let body = physics.add_static(&placed.collider);
                        track_collision_body(
                            &mut self.collision_references,
                            &mut self.collision_bodies,
                            body,
                            placed.reference,
                        );
                        bodies.push(body);
                    }
                    streamer
                        .bind_mut()
                        .finish_streamed_upload(&uploading.cell, &game.world());
                    exterior.cells.insert(id, bodies);
                    exterior.planner.mark_drawable(uploading.request);
                    entered.push(id);
                }
            } else {
                streamer
                    .bind_mut()
                    .apply_cell_delta(&[], &[id], &game.world());
                for placed in &uploading.cell.colliders[..uploading.placed] {
                    self.query_collision.remove_reference(placed.reference);
                }
            }
        }

        let departed: Vec<_> = plan
            .retirements
            .iter()
            .filter_map(|request| {
                source
                    .cell_index()
                    .exterior_cell(exterior.world, request.cell.x, request.cell.y)
            })
            .collect();
        if !departed.is_empty()
            && !streamer
                .bind_mut()
                .apply_cell_delta(&[], &departed, &game.world())
        {
            for request in &plan.retirements {
                exterior.planner.mark_retirement_failed(*request);
            }
            return false;
        }
        for id in &departed {
            if let Some(bodies) = exterior.cells.remove(id) {
                for body in bodies {
                    if let Some(reference) = self.collision_references.get(&body) {
                        self.query_collision.remove_reference(*reference);
                    }
                    physics.remove(body);
                    untrack_collision_body(
                        &mut self.collision_references,
                        &mut self.collision_bodies,
                        body,
                    );
                }
            }
        }
        for request in &plan.retirements {
            exterior.planner.mark_retired(*request);
        }
        if !entered.is_empty() || !departed.is_empty() || self.loaded_cell != Some(focus_cell) {
            if let Err(error) = game.update_streaming_cells(focus_cell, &entered, &departed) {
                soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior residency update: {error}");
                return false;
            }
        }
        game.record_exterior_cell_loads(entered.len());
        if !streamer.bind_mut().update_distant(
            exterior.world,
            [eye.x, eye.y, eye.z],
            &game.world(),
            true,
        ) {
            return false;
        }
        let audio_changed = self.loaded_cell != Some(focus_cell);
        self.loaded_cell = Some(focus_cell);
        if audio_changed {
            self.sync_cell_ambience(focus_cell);
            self.sync_cell_music(focus_cell);
        }
        true
    }
}

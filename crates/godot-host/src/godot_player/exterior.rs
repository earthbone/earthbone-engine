// SPDX-License-Identifier: GPL-3.0-or-later

//! Walking grid deltas follow OpenMW Scene::changeCellGrid and the host's
//! hysteresis planner. Render and physics resources retain their CELL owners.

use std::collections::{BTreeMap, VecDeque};
use std::thread::JoinHandle;

use godot::obj::{Singleton, WithBaseField};
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
            crate::batch::PreparedOccluder,
            Vec<Option<crate::godot_physics::PreparedBody>>,
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
    retirements: VecDeque<Retiring>,
    lifecycle: Option<CellDelta>,
}

struct CellDelta {
    entered: Vec<GlobalFormId>,
    departed: Vec<GlobalFormId>,
}

struct Retiring {
    request: CellRequest,
    cell: GlobalFormId,
    started: bool,
    bodies: Vec<BodyId>,
}

struct Uploading {
    request: CellRequest,
    cell: crate::data_source::PreparedCell,
    plan: crate::godot_layer::upload::Plan,
    totals: crate::godot_layer::StreamTotals,
    placed: usize,
    query_ready: bool,
    query_bodies: Vec<Option<crate::godot_physics::PreparedBody>>,
    bodies: Vec<BodyId>,
    occluder: crate::batch::PreparedOccluder,
    movable: crate::godot_physics::MovableGroups,
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

    let first = if progressive {
        godot::classes::Engine::singleton().get_process_frames() % 3
    } else {
        0
    };
    for offset in 0..3 {
        let drawn = match (first + offset) % 3 {
            0 => node.update_terrain_lod(focus.x, focus.y),
            1 => node.update_object_lod(focus.x, focus.y),
            _ => node.update_tree_lod(focus.x, focus.y),
        };
        if drawn < 0 {
            return false;
        }
    }
    true
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
        let started = std::time::Instant::now();
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
        let Some(focus_cell) = source
            .cell_index()
            .exterior_cell(exterior.world, coord.x, coord.y)
        else {
            return false;
        };
        if let Some(delta) = exterior.pending.lifecycle.take() {
            let _span = soe_trace::Span::new("host.exterior_game_cells", "host");
            if let Err(error) =
                game.update_streaming_cells(focus_cell, &delta.entered, &delta.departed)
            {
                soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior residency update: {error}");
                return false;
            }
            game.record_exterior_cell_loads(delta.entered.len());
            let audio_changed = self.loaded_cell != Some(focus_cell);
            self.loaded_cell = Some(focus_cell);
            if audio_changed {
                self.sync_cell_ambience(focus_cell);
                self.sync_cell_music(focus_cell);
            }
            return true;
        }
        streamer.bind_mut().retire_uploads_step();
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
        let planning_span = soe_trace::Span::new("host.exterior_plan", "host");
        let busy = exterior.pending.job.is_some()
            || !exterior.pending.queue.is_empty()
            || exterior.pending.upload.is_some()
            || !exterior.pending.retirements.is_empty();
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
        drop(planning_span);
        for request in &plan.retirements {
            if let Some(cell) =
                source
                    .cell_index()
                    .exterior_cell(exterior.world, request.cell.x, request.cell.y)
            {
                exterior.pending.retirements.push_back(Retiring {
                    request: *request,
                    cell,
                    started: false,
                    bodies: Vec::new(),
                });
            } else {
                exterior.planner.mark_retired(*request);
            }
        }
        if !plan.requests.is_empty() {
            for request in &plan.requests {
                exterior.planner.mark_loading(*request);
            }
            let source = std::sync::Arc::clone(source);
            let world = exterior.world;
            let requests = plan.requests.clone();
            let batch = streamer.bind().batch_uploads();
            let enabled_span = soe_trace::Span::new("host.exterior_enabled", "host");
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
            drop(enabled_span);
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
                                            &cell.loaded.particles,
                                        );
                                        let _span = soe_trace::Span::new(
                                            "host.exterior_occluder_prepare",
                                            "host",
                                        );
                                        let surfaces: Vec<_> = cell
                                            .loaded
                                            .surfaces
                                            .iter()
                                            .filter(|surface| {
                                                crate::godot_layer::upload::base_tag(
                                                    Some(&source),
                                                    surface.reference,
                                                ) != Some(soe_esm::Tag::new(b"DOOR"))
                                            })
                                            .collect();
                                        let (vertices, indices) =
                                            crate::batch::cell_occluder_mesh_refs(
                                                surfaces.iter().copied(),
                                            );
                                        let sources = surfaces
                                            .iter()
                                            .map(|surface| (surface.reference, surface.placement))
                                            .collect();
                                        let occluder = crate::batch::PreparedOccluder {
                                            vertices,
                                            indices,
                                            sources,
                                        };
                                        let query_bodies = cell
                                            .colliders
                                            .iter()
                                            .map(|placed| {
                                                placed.collider.is_static().then(|| {
                                                    crate::godot_physics::prepare_body(
                                                        &placed.collider,
                                                    )
                                                })
                                            })
                                            .collect();
                                        (cell, plan, occluder, query_bodies)
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
                    Ok(Some((cell, upload, occluder, query_bodies))) => {
                        exterior.planner.mark_completed(request);
                        exterior.pending.upload = Some(Uploading {
                            request,
                            movable: crate::godot_physics::movable_groups(&cell.colliders),
                            cell,
                            plan: upload,
                            totals: crate::godot_layer::StreamTotals::default(),
                            placed: 0,
                            query_ready: false,
                            query_bodies,
                            bodies: Vec::new(),
                            occluder,
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
        let retire_started = std::time::Instant::now();
        let retire_span = soe_trace::Span::new("host.exterior_retire", "host");
        let mut departed = Vec::new();
        while started.elapsed() < crate::godot_layer::upload::SLICE_BUDGET {
            let Some(mut retiring) = exterior.pending.retirements.pop_front() else {
                break;
            };
            if !retiring.started {
                if exterior.planner.request(retiring.request.cell) != Some(retiring.request)
                    || exterior.planner.state(retiring.request.cell)
                        != Some(crate::streaming::CellState::Retiring)
                {
                    continue;
                }
                let detach_span = soe_trace::Span::new("host.exterior_retire_detach", "host");
                if !streamer
                    .bind_mut()
                    .retire_streamed_cell(retiring.cell, &game.world())
                {
                    exterior.planner.mark_retirement_failed(retiring.request);
                    return false;
                }
                drop(detach_span);
                if !exterior.planner.begin_retirement(retiring.request) {
                    return false;
                }
                retiring.bodies = exterior.cells.remove(&retiring.cell).unwrap_or_default();
                retiring.started = true;
            }
            while started.elapsed() < crate::godot_layer::upload::SLICE_BUDGET {
                let Some(body) = retiring.bodies.pop() else {
                    break;
                };
                if let Some(reference) = self.collision_references.get(&body) {
                    let _span = soe_trace::Span::new("host.exterior_retire_query", "host");
                    self.query_collision.remove_reference(*reference);
                }
                let controller_span =
                    soe_trace::Span::new("host.exterior_retire_controller", "host");
                physics.remove(body);
                drop(controller_span);
                untrack_collision_body(
                    &mut self.collision_references,
                    &mut self.collision_bodies,
                    body,
                );
            }
            if retiring.bodies.is_empty() && !streamer.bind().cell_release_pending(retiring.cell) {
                exterior.planner.mark_retired(retiring.request);
                departed.push(retiring.cell);
            } else {
                exterior.pending.retirements.push_front(retiring);
                break;
            }
        }
        drop(retire_span);
        streamer
            .bind_mut()
            .charge_streaming_budget(retire_started.elapsed());
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
                    let collision_started = std::time::Instant::now();
                    let _span = soe_trace::Span::new("host.exterior_query_collision", "host");
                    while uploading.placed < uploading.cell.colliders.len()
                        && started.elapsed() < crate::godot_layer::upload::SLICE_BUDGET
                    {
                        let placed = &uploading.cell.colliders[uploading.placed];
                        if placed.collider.is_static() {
                            let _span = soe_trace::Span::new("host.exterior_query_body", "host");
                            if !uploading.query_ready {
                                let Some(prepared) =
                                    uploading.query_bodies[uploading.placed].take()
                                else {
                                    return false;
                                };
                                self.query_collision.append(
                                    crate::godot_physics::add_prepared_query_static(
                                        space, placed, prepared,
                                    ),
                                );
                                uploading.query_ready = true;
                            }
                            if started.elapsed() >= crate::godot_layer::upload::SLICE_BUDGET {
                                break;
                            }
                            let body = physics.add_static(&placed.collider);
                            track_collision_body(
                                &mut self.collision_references,
                                &mut self.collision_bodies,
                                body,
                                placed.reference,
                            );
                            uploading.bodies.push(body);
                        }
                        uploading.query_ready = false;
                        uploading.placed += 1;
                    }
                    streamer
                        .bind_mut()
                        .charge_streaming_budget(collision_started.elapsed());
                }
                if !uploading.plan.is_empty()
                    || uploading.placed < uploading.cell.colliders.len()
                    || started.elapsed() >= crate::godot_layer::upload::SLICE_BUDGET
                {
                    exterior.pending.upload = Some(uploading);
                } else if uploading.movable.is_empty() {
                    let finalize_started = std::time::Instant::now();
                    let finalize_span = soe_trace::Span::new("host.exterior_finalize", "host");
                    streamer.bind_mut().finish_streamed_upload(
                        &uploading.cell,
                        &game.world(),
                        std::mem::take(&mut uploading.occluder),
                    );
                    exterior
                        .cells
                        .insert(id, std::mem::take(&mut uploading.bodies));
                    exterior.planner.mark_drawable(uploading.request);
                    entered.push(id);
                    drop(finalize_span);
                    streamer
                        .bind_mut()
                        .charge_streaming_budget(finalize_started.elapsed());
                } else {
                    let movable_started = std::time::Instant::now();
                    if !streamer.bind_mut().install_movable_groups_step(
                        &mut uploading.movable,
                        crate::godot_layer::upload::SLICE_BUDGET.saturating_sub(started.elapsed()),
                    ) {
                        return false;
                    }
                    streamer
                        .bind_mut()
                        .charge_streaming_budget(movable_started.elapsed());
                    exterior.pending.upload = Some(uploading);
                }
            } else {
                streamer
                    .bind_mut()
                    .apply_cell_delta(&[], &[id], &game.world());
                let query_count = uploading.placed + usize::from(uploading.query_ready);
                for placed in &uploading.cell.colliders[..query_count] {
                    self.query_collision.remove_reference(placed.reference);
                }
                for body in uploading.bodies {
                    physics.remove(body);
                    untrack_collision_body(
                        &mut self.collision_references,
                        &mut self.collision_bodies,
                        body,
                    );
                }
            }
        }

        if !entered.is_empty() || !departed.is_empty() {
            exterior.pending.lifecycle = Some(CellDelta { entered, departed });
            return true;
        }
        if self.loaded_cell != Some(focus_cell) {
            let _span = soe_trace::Span::new("host.exterior_game_cells", "host");
            if let Err(error) = game.update_streaming_cells(focus_cell, &[], &[]) {
                soe_trace::error!(target: "host::exterior", system = "cell_streaming", "exterior residency update: {error}");
                return false;
            }
        }
        let distant_span = soe_trace::Span::new("host.exterior_distant", "host");
        if !streamer.bind_mut().update_distant(
            exterior.world,
            [eye.x, eye.y, eye.z],
            &game.world(),
            true,
        ) {
            return false;
        }
        drop(distant_span);
        let audio_changed = self.loaded_cell != Some(focus_cell);
        self.loaded_cell = Some(focus_cell);
        if audio_changed {
            self.sync_cell_ambience(focus_cell);
            self.sync_cell_music(focus_cell);
        }
        true
    }
}

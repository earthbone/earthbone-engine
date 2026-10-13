// SPDX-License-Identifier: GPL-3.0-or-later

//! Which exterior cells to have loaded around a focus, and in what order.
//!
//! The game keeps a square of `uGridsToLoad` cells (5 by default) around the
//! player and swaps rows as the player crosses a cell boundary. This keeps
//! the same shape but adds two things the original lacks: a wider unload
//! radius than load radius so a player pacing on a boundary does not thrash,
//! and a per-tick budget so a fast traversal spreads work over frames
//! instead of stalling one. Lifecycle and demand ordering follow Mudcrab main
//! 268f988 and PR 210. The streamer only decides. Loading is the
//! caller's, through the plan it returns.

mod priority;

use crate::units::{SkyrimPos, CELL_UNITS};

/// An exterior cell in a worldspace's grid. Cell (0, 0) spans units 0 to
/// 4095 on both axes, negative cells go west and south.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The cell containing a world position.
    pub fn containing(pos: SkyrimPos) -> Self {
        Self {
            x: (pos.x / CELL_UNITS).floor() as i32,
            y: (pos.y / CELL_UNITS).floor() as i32,
        }
    }

    /// Chebyshev distance in cells, the metric the load square uses.
    pub fn ring_distance(self, other: CellCoord) -> i32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }

    /// Squared Euclidean distance in cells, for ordering within a ring.
    pub fn distance_sq(self, other: CellCoord) -> i64 {
        let dx = (self.x - other.x) as i64;
        let dy = (self.y - other.y) as i64;
        dx * dx + dy * dy
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    /// Cells on each side of the focus cell to keep loaded. 2 is the game's
    /// `uGridsToLoad=5`.
    pub load_radius: i32,
    /// Cells on each side beyond which a loaded cell is released. Must be at
    /// least `load_radius`; the gap is the hysteresis band.
    pub unload_radius: i32,
    /// Most cells to start loading in one `update`. 0 means unlimited.
    pub max_loads_per_tick: usize,
    /// Most cells to release in one `update`. 0 means unlimited.
    pub max_unloads_per_tick: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            load_radius: 2,
            unload_radius: 3,
            max_loads_per_tick: 2,
            max_unloads_per_tick: 4,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("load radius {0} is negative")]
    NegativeRadius(i32),
    #[error("unload radius {unload} is smaller than load radius {load}")]
    UnloadInsideLoad { load: i32, unload: i32 },
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.load_radius < 0 {
            return Err(ConfigError::NegativeRadius(self.load_radius));
        }
        if self.unload_radius < self.load_radius {
            return Err(ConfigError::UnloadInsideLoad {
                load: self.load_radius,
                unload: self.unload_radius,
            });
        }
        Ok(())
    }
}

/// What one update asks the caller to do. Loads follow current demand priority,
/// and acknowledged drawable retirements go farthest first.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StreamPlan {
    pub load: Vec<CellCoord>,
    pub unload: Vec<CellCoord>,
    /// Attempt tokens for generation-checked preparation responses.
    pub requests: Vec<CellRequest>,
    /// Tokens for acknowledged resource retirement.
    pub retirements: Vec<CellRequest>,
}

/// Measured or estimated render work for one cell load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamCost {
    pub triangles: usize,
    /// Mesh array bytes plus texture payload bytes required by the cell.
    pub bytes: usize,
}

/// Maximum render work selected for one streaming tick. A zero limit is
/// unlimited, matching the cell-count limits in [`StreamConfig`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamBudget {
    pub triangles: usize,
    pub bytes: usize,
}

impl StreamPlan {
    pub fn is_empty(&self) -> bool {
        self.load.is_empty() && self.unload.is_empty()
    }
}

/// A generation identifies one attempt, including retries after cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRequest {
    pub cell: CellCoord,
    pub generation: u64,
}

// Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab,
// crates/engine/src/streaming.rs at 268f988), MIT License,
// Copyright (c) the Mudcrab contributors
/// Presentation lifecycle. Sending a request never establishes residency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Requested,
    Loading,
    Drawable,
    Failed,
    Retiring,
}

const MAX_LOAD_ATTEMPTS: u32 = 3;
static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Debug, Clone)]
struct CellEntry {
    generation: u64,
    state: CellState,
    attempts: u32,
    retry_at: u64,
    completed: bool,
    requested_at: std::time::Instant,
}

// Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab,
// crates/engine/src/streaming.rs at 268f988), MIT License,
// Copyright (c) the Mudcrab contributors
/// Generation-checked cell demand and acknowledged presentation ownership.
#[derive(Debug, Clone)]
pub struct CellStreamer {
    config: StreamConfig,
    cells: std::collections::BTreeMap<CellCoord, CellEntry>,
    last_focus: Option<SkyrimPos>,
    view: Option<[f64; 3]>,
    movement: [f32; 2],
    tick: u64,
    next_generation: u64,
    next_sequence: u64,
    served: u64,
    record_trace: bool,
    desired: Vec<CellCoord>,
    queued_age: std::collections::BTreeMap<CellCoord, u64>,
}

impl CellStreamer {
    pub fn new(config: StreamConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            cells: std::collections::BTreeMap::new(),
            last_focus: None,
            view: None,
            movement: [0.0; 2],
            tick: 0,
            next_generation: 0,
            next_sequence: 0,
            served: 0,
            record_trace: true,
            desired: Vec::new(),
            queued_age: std::collections::BTreeMap::new(),
        })
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Stable Skyrim-space camera forward. Pending demand is re-ranked on turns.
    pub fn set_view(&mut self, forward: [f64; 3]) {
        self.view = Some(forward);
    }

    /// Drawable resources, including those awaiting retirement acknowledgement.
    pub fn resident(&self) -> impl Iterator<Item = CellCoord> + '_ {
        self.cells.iter().filter_map(|(cell, entry)| {
            matches!(entry.state, CellState::Drawable | CellState::Retiring).then_some(*cell)
        })
    }

    pub fn resident_count(&self) -> usize {
        self.resident().count()
    }

    pub fn is_resident(&self, cell: CellCoord) -> bool {
        matches!(
            self.state(cell),
            Some(CellState::Drawable | CellState::Retiring)
        )
    }

    /// Current lifecycle, including terminal failures.
    pub fn state(&self, cell: CellCoord) -> Option<CellState> {
        self.cells.get(&cell).map(|entry| entry.state)
    }

    /// Token for the current request. Capture it before starting worker work.
    pub fn request(&self, cell: CellCoord) -> Option<CellRequest> {
        self.cells.get(&cell).map(|entry| CellRequest {
            cell,
            generation: entry.generation,
        })
    }

    fn next_cell(&self, focus: SkyrimPos) -> Option<CellCoord> {
        let movement = self
            .last_focus
            .map(|previous| [focus.x - previous.x, focus.y - previous.y])
            .filter(|movement| movement.iter().all(|v| v.is_finite()) && *movement != [0.0; 2])
            .unwrap_or(self.movement);
        let length = movement[0].hypot(movement[1]);
        (length.is_finite() && length > 0.0).then(|| {
            CellCoord::containing(SkyrimPos::new(
                focus.x + movement[0] / length * CELL_UNITS,
                focus.y + movement[1] / length * CELL_UNITS,
                focus.z,
            ))
        })
    }

    fn demand(&self, focus: SkyrimPos) -> Vec<CellCoord> {
        let centre = CellCoord::containing(focus);
        let r = self.config.load_radius;
        let mut wanted = Vec::new();
        for y in (centre.y - r)..=(centre.y + r) {
            for x in (centre.x - r)..=(centre.x + r) {
                wanted.push(CellCoord::new(x, y));
            }
        }
        if let Some(next) = self.next_cell(focus) {
            if !wanted.contains(&next) {
                wanted.push(next);
            }
        }
        wanted.sort_by_key(|cell| {
            let distance = (i64::from(cell.x) - i64::from(centre.x)).abs()
                + (i64::from(cell.y) - i64::from(centre.y)).abs();
            (
                distance,
                i64::from(cell.x).abs() + i64::from(cell.y).abs(),
                cell.x,
                cell.y,
            )
        });
        wanted
    }

    /// Next-update unstarted demand, including failures due on that planning tick.
    pub fn candidates(&self, focus: SkyrimPos) -> Vec<CellCoord> {
        self.candidates_at(focus, self.tick.saturating_add(1))
    }

    fn candidates_at(&self, focus: SkyrimPos, tick: u64) -> Vec<CellCoord> {
        self.demand(focus)
            .into_iter()
            .filter(|cell| {
                self.cells.get(cell).is_none_or(|entry| {
                    entry.state == CellState::Failed
                        && entry.attempts < MAX_LOAD_ATTEMPTS
                        && tick >= entry.retry_at
                })
            })
            .collect()
    }

    /// Explicitly reset a dropped cell or an exhausted failure after repair.
    pub fn mark_unloaded(&mut self, cell: CellCoord) {
        self.cells.remove(&cell);
        self.publish_coverage();
    }

    fn matching(&mut self, request: CellRequest, state: CellState) -> Option<&mut CellEntry> {
        let valid = self
            .cells
            .get(&request.cell)
            .is_some_and(|entry| entry.generation == request.generation && entry.state == state);
        if !valid {
            soe_trace::streaming_transition(soe_trace::StreamingTransition::StaleResponse);
            return None;
        }
        self.cells.get_mut(&request.cell)
    }

    /// A caller accepted the request and started CPU preparation.
    pub fn mark_loading(&mut self, request: CellRequest) -> bool {
        let Some(entry) = self.matching(request, CellState::Requested) else {
            return false;
        };
        entry.state = CellState::Loading;
        true
    }

    /// CPU preparation completed. This does not establish drawable coverage.
    pub fn mark_completed(&mut self, request: CellRequest) -> bool {
        let Some(entry) = self.matching(request, CellState::Loading) else {
            return false;
        };
        if entry.completed {
            return false;
        }
        entry.completed = true;
        soe_trace::streaming_transition(soe_trace::StreamingTransition::Completed);
        true
    }

    /// Renderer accepted the prepared replacement. No GPU completion is inferred.
    pub fn mark_drawable(&mut self, request: CellRequest) -> bool {
        let Some(entry) = self.matching(request, CellState::Loading) else {
            return false;
        };
        if !entry.completed {
            return false;
        }
        entry.state = CellState::Drawable;
        let elapsed = entry.requested_at.elapsed();
        soe_trace::info!(target: "host::streaming", system = "cell_streaming",
            "CELL_DRAWABLE x={} y={} generation={} latency_us={}",
            request.cell.x, request.cell.y, request.generation, elapsed.as_micros());
        self.publish_coverage();
        true
    }

    /// Failure retains the request identity and schedules at most two retries.
    pub fn mark_failed(&mut self, request: CellRequest) -> bool {
        let retry_tick = self.tick;
        let Some(entry) = self.cells.get_mut(&request.cell) else {
            soe_trace::streaming_transition(soe_trace::StreamingTransition::StaleResponse);
            return false;
        };
        if entry.generation != request.generation
            || !matches!(entry.state, CellState::Requested | CellState::Loading)
        {
            soe_trace::streaming_transition(soe_trace::StreamingTransition::StaleResponse);
            return false;
        }
        entry.state = CellState::Failed;
        entry.retry_at = retry_tick.saturating_add(1_u64 << entry.attempts);
        soe_trace::streaming_transition(soe_trace::StreamingTransition::Failed);
        true
    }

    /// A matching retirement finished freeing its host resources.
    pub fn mark_retired(&mut self, request: CellRequest) -> bool {
        if self.matching(request, CellState::Retiring).is_none() {
            return false;
        }
        self.cells.remove(&request.cell);
        self.publish_coverage();
        true
    }

    /// Failed host retirement leaves drawable ownership available for a later attempt.
    pub fn mark_retirement_failed(&mut self, request: CellRequest) -> bool {
        if self.matching(request, CellState::Retiring).is_none() {
            return false;
        }
        self.next_generation = if self.record_trace {
            NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        } else {
            self.next_generation.saturating_add(1)
        };
        if let Some(entry) = self.cells.get_mut(&request.cell) {
            entry.state = CellState::Drawable;
            entry.generation = self.next_generation;
        }
        true
    }

    fn publish_coverage(&self) {
        if !self.record_trace {
            return;
        }
        soe_trace::streaming_coverage(
            self.desired.len() as u64,
            self.desired
                .iter()
                .filter(|cell| self.is_resident(**cell))
                .count() as u64,
            self.resident_count() as u64,
        );
    }

    /// Preview the exact count-limited intake without admitting requests or tracing them.
    pub fn preview_loads(&self, focus: SkyrimPos) -> Vec<CellCoord> {
        let mut preview = self.clone();
        preview.record_trace = false;
        preview.update(focus).load
    }

    pub fn update(&mut self, focus: SkyrimPos) -> StreamPlan {
        self.update_inner(focus, None)
    }

    /// Fixed count, triangle and byte limits. Indivisible oversized work is deferred.
    pub fn update_with_budget(
        &mut self,
        focus: SkyrimPos,
        budget: StreamBudget,
        mut cost_of: impl FnMut(CellCoord) -> StreamCost,
    ) -> StreamPlan {
        self.update_inner(focus, Some((budget, &mut cost_of)))
    }

    fn update_inner(
        &mut self,
        focus: SkyrimPos,
        mut budgeted: Option<(StreamBudget, &mut dyn FnMut(CellCoord) -> StreamCost)>,
    ) -> StreamPlan {
        self.tick = self.tick.saturating_add(1);
        if let Some(previous) = self.last_focus {
            let movement = [focus.x - previous.x, focus.y - previous.y];
            if movement.iter().all(|v| v.is_finite()) && movement != [0.0; 2] {
                self.movement = movement;
            }
        }
        self.last_focus = Some(focus);
        let centre = CellCoord::containing(focus);
        self.desired = self.demand(focus);
        let desired = &self.desired;
        self.cells.retain(|cell, entry| {
            let relevant =
                desired.contains(cell) || cell.ring_distance(centre) <= self.config.unload_radius;
            if relevant && entry.state == CellState::Retiring {
                entry.state = CellState::Drawable;
                self.next_generation = if self.record_trace {
                    NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                } else {
                    self.next_generation.saturating_add(1)
                };
                entry.generation = self.next_generation;
            }
            relevant || matches!(entry.state, CellState::Drawable | CellState::Retiring)
        });
        self.queued_age
            .retain(|cell, _| self.desired.contains(cell));
        let wanted = self.candidates_at(focus, self.tick);
        let mut priorities = Vec::new();
        for cell in &wanted {
            let sequence = *self.queued_age.entry(*cell).or_insert_with(|| {
                self.next_sequence = self.next_sequence.saturating_add(1);
                self.next_sequence
            });
            priorities.push(priority::DemandPriority::new(
                priority::PriorityView {
                    position: [f64::from(focus.x), f64::from(focus.y), 0.0],
                    forward: self.view.unwrap_or([0.0; 3]),
                },
                [
                    (f64::from(cell.x) + 0.5) * f64::from(CELL_UNITS),
                    (f64::from(cell.y) + 0.5) * f64::from(CELL_UNITS),
                    0.0,
                ],
                Some(f64::from(CELL_UNITS) / 2.0_f64.sqrt()),
                true,
                sequence,
            ));
        }
        let mut remaining: Vec<_> = (0..wanted.len()).collect();
        let mut plan = StreamPlan::default();
        let mut used = StreamCost::default();
        while !remaining.is_empty() {
            if self.config.max_loads_per_tick > 0
                && plan.load.len() >= self.config.max_loads_per_tick
            {
                break;
            }
            let selected = if self.view.is_some() {
                remaining
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        if self.served % priority::AGED_SERVICE_INTERVAL
                            == priority::AGED_SERVICE_INTERVAL - 1
                        {
                            priorities[**a].compare_age(&priorities[**b])
                        } else {
                            let next = self.next_cell(focus);
                            let urgent = |index: usize| {
                                if wanted[index] == centre {
                                    2
                                } else {
                                    i32::from(Some(wanted[index]) == next)
                                }
                            };
                            urgent(**b)
                                .cmp(&urgent(**a))
                                .then_with(|| priorities[**a].compare(&priorities[**b]))
                        }
                    })
                    .map_or(0, |(index, _)| index)
            } else {
                0
            };
            let index = remaining.remove(selected);
            let cell = wanted[index];
            let cost = budgeted
                .as_mut()
                .map(|(_, cost_of)| cost_of(cell))
                .unwrap_or_default();
            let fits = budgeted.as_ref().is_none_or(|(budget, _)| {
                (budget.triangles == 0
                    || cost.triangles <= budget.triangles.saturating_sub(used.triangles))
                    && (budget.bytes == 0 || cost.bytes <= budget.bytes.saturating_sub(used.bytes))
            });
            if !fits {
                continue;
            }
            used.triangles = used.triangles.saturating_add(cost.triangles);
            used.bytes = used.bytes.saturating_add(cost.bytes);
            self.served = self.served.saturating_add(1);
            self.next_generation = if self.record_trace {
                NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            } else {
                self.next_generation.saturating_add(1)
            };
            let attempts = self.cells.get(&cell).map_or(1, |entry| entry.attempts + 1);
            self.cells.insert(
                cell,
                CellEntry {
                    generation: self.next_generation,
                    state: CellState::Requested,
                    attempts,
                    retry_at: 0,
                    completed: false,
                    requested_at: std::time::Instant::now(),
                },
            );
            plan.load.push(cell);
            plan.requests.push(CellRequest {
                cell,
                generation: self.next_generation,
            });
            if self.record_trace {
                soe_trace::streaming_transition(soe_trace::StreamingTransition::Requested);
            }
        }
        // Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab,
        // docs/research/streaming-priority.md at PR 210), MIT License,
        // Copyright (c) the Mudcrab contributors
        // CONTEXT: Old drawables bridge the replacement's preparation and upload.
        if self
            .desired
            .iter()
            .all(|cell| self.state(*cell) == Some(CellState::Drawable))
        {
            let mut stale: Vec<_> = self
                .cells
                .iter()
                .filter_map(|(cell, entry)| {
                    (entry.state == CellState::Drawable
                        && !self.desired.contains(cell)
                        && cell.ring_distance(centre) > self.config.unload_radius)
                        .then_some(*cell)
                })
                .collect();
            stale.sort_by_key(|cell| (std::cmp::Reverse(cell.distance_sq(centre)), cell.x, cell.y));
            if self.config.max_unloads_per_tick > 0 {
                stale.truncate(self.config.max_unloads_per_tick);
            }
            for cell in stale {
                if let Some(entry) = self.cells.get_mut(&cell) {
                    entry.state = CellState::Retiring;
                    plan.unload.push(cell);
                    plan.retirements.push(CellRequest {
                        cell,
                        generation: entry.generation,
                    });
                }
            }
        }
        self.publish_coverage();
        plan
    }

    /// Drain request planning. Callers still acknowledge each actual load and unload.
    pub fn settle(&mut self, focus: SkyrimPos) -> StreamPlan {
        self.movement = [0.0; 2];
        self.last_focus = Some(focus);
        let mut total = StreamPlan::default();
        loop {
            let plan = self.update(focus);
            if plan.is_empty() {
                return total;
            }
            total.load.extend(plan.load);
            total.unload.extend(plan.unload);
            total.requests.extend(plan.requests);
            total.retirements.extend(plan.retirements);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlimited(load: i32, unload: i32) -> CellStreamer {
        CellStreamer::new(StreamConfig {
            load_radius: load,
            unload_radius: unload,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 0,
        })
        .unwrap()
    }

    #[test]
    fn cell_of_a_position_floors_toward_negative() {
        assert_eq!(
            CellCoord::containing(SkyrimPos::new(0.0, 0.0, 0.0)),
            CellCoord::new(0, 0)
        );
        assert_eq!(
            CellCoord::containing(SkyrimPos::new(4095.9, 4096.0, 0.0)),
            CellCoord::new(0, 1)
        );
        assert_eq!(
            CellCoord::containing(SkyrimPos::new(-0.5, -4096.5, 0.0)),
            CellCoord::new(-1, -2)
        );
    }

    #[test]
    fn a_fresh_streamer_loads_the_full_square_nearest_first() {
        let mut s = unlimited(2, 3);
        let plan = s.update(SkyrimPos::new(100.0, 100.0, 0.0));
        assert_eq!(plan.load.len(), 25);
        assert_eq!(plan.unload.len(), 0);
        assert_eq!(plan.load[0], CellCoord::new(0, 0));
        // The four edge-adjacent cells come before any diagonal.
        for c in &plan.load[1..5] {
            assert_eq!(c.distance_sq(CellCoord::new(0, 0)), 1, "{c:?}");
        }
        assert_eq!(s.resident_count(), 0);
        acknowledge(&mut s, &plan);
        assert_eq!(s.resident_count(), 25);
    }

    #[test]
    fn the_budget_caps_loads_per_tick_and_settle_drains_it() {
        let mut s = CellStreamer::new(StreamConfig {
            max_loads_per_tick: 3,
            ..StreamConfig::default()
        })
        .unwrap();
        let first = s.update(SkyrimPos::default());
        assert_eq!(first.load.len(), 3);
        acknowledge(&mut s, &first);
        let rest = s.settle(SkyrimPos::default());
        assert_eq!(rest.load.len(), 22);
        acknowledge(&mut s, &rest);
        assert!(s.update(SkyrimPos::default()).is_empty());
    }

    #[test]
    fn crossing_one_boundary_loads_a_column_and_unloads_nothing_inside_hysteresis() {
        let mut s = unlimited(2, 3);
        settle_drawable(&mut s, SkyrimPos::default());
        // One cell east: new column x=3 wanted, column x=-2 is now at ring 3,
        // inside the unload radius, so it stays.
        let plan = s.update(SkyrimPos::new(CELL_UNITS + 1.0, 0.0, 0.0));
        assert_eq!(plan.load.len(), 5);
        assert!(plan.load.iter().all(|c| c.x == 3));
        assert_eq!(plan.unload.len(), 0);
        acknowledge(&mut s, &plan);
        assert_eq!(s.resident_count(), 30);
        // Two cells east: column x=-2 is at ring 4 and goes.
        let plan = s.update(SkyrimPos::new(2.0 * CELL_UNITS + 1.0, 0.0, 0.0));
        assert!(plan.unload.is_empty());
        acknowledge(&mut s, &plan);
        let plan = s.update(SkyrimPos::new(2.0 * CELL_UNITS + 1.0, 0.0, 0.0));
        assert_eq!(plan.unload.len(), 5);
        assert!(plan.unload.iter().all(|c| c.x == -2));
        acknowledge(&mut s, &plan);
        assert_eq!(s.resident_count(), 30);
    }

    #[test]
    fn pacing_on_a_boundary_does_not_thrash() {
        let mut s = unlimited(2, 3);
        settle_drawable(&mut s, SkyrimPos::default());
        let mut unloads = 0;
        for i in 0..20 {
            let x = if i % 2 == 0 { -1.0 } else { 1.0 };
            let plan = s.update(SkyrimPos::new(x, 0.0, 0.0));
            unloads += plan.unload.len();
            acknowledge(&mut s, &plan);
        }
        assert_eq!(unloads, 0);
        assert_eq!(s.resident_count(), 30);
    }

    #[test]
    fn unloads_go_farthest_first_under_a_budget() {
        let mut s = CellStreamer::new(StreamConfig {
            load_radius: 1,
            unload_radius: 1,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 1,
        })
        .unwrap();
        settle_drawable(&mut s, SkyrimPos::default());
        let far = SkyrimPos::new(10.0 * CELL_UNITS, 10.0 * CELL_UNITS, 0.0);
        let plan = s.update(far);
        assert!(plan.unload.is_empty());
        acknowledge(&mut s, &plan);
        let plan = s.update(far);
        assert_eq!(plan.unload, vec![CellCoord::new(-1, -1)]);
    }

    #[test]
    fn a_failed_load_is_retried_after_mark_unloaded() {
        let mut s = unlimited(0, 0);
        assert_eq!(
            s.update(SkyrimPos::default()).load,
            vec![CellCoord::new(0, 0)]
        );
        assert_eq!(s.update(SkyrimPos::default()).load.len(), 0);
        s.mark_unloaded(CellCoord::new(0, 0));
        assert_eq!(
            s.update(SkyrimPos::default()).load,
            vec![CellCoord::new(0, 0)]
        );
    }

    #[test]
    fn bad_configs_are_rejected() {
        let bad = StreamConfig {
            load_radius: 3,
            unload_radius: 2,
            ..StreamConfig::default()
        };
        assert_eq!(
            CellStreamer::new(bad).err(),
            Some(ConfigError::UnloadInsideLoad { load: 3, unload: 2 })
        );
        let neg = StreamConfig {
            load_radius: -1,
            ..StreamConfig::default()
        };
        assert_eq!(
            CellStreamer::new(neg).err(),
            Some(ConfigError::NegativeRadius(-1))
        );
    }

    fn acknowledge(s: &mut CellStreamer, plan: &StreamPlan) {
        for request in &plan.requests {
            assert!(s.mark_loading(*request));
            assert!(s.mark_completed(*request));
            assert!(s.mark_drawable(*request));
        }
        for request in &plan.retirements {
            assert!(s.mark_retired(*request));
        }
    }

    fn settle_drawable(s: &mut CellStreamer, focus: SkyrimPos) {
        let requests = s.settle(focus);
        acknowledge(s, &requests);
        let retirements = s.update(focus);
        acknowledge(s, &retirements);
    }

    #[test]
    fn requested_and_cpu_completed_cells_are_not_resident() {
        let mut s = unlimited(0, 0);
        let request = s.update(SkyrimPos::default()).requests[0];
        assert_eq!(s.state(request.cell), Some(CellState::Requested));
        assert_eq!(s.resident_count(), 0);
        assert!(s.mark_loading(request));
        assert!(s.mark_completed(request));
        assert_eq!(s.resident_count(), 0);
        assert!(s.mark_drawable(request));
        assert_eq!(s.resident_count(), 1);
    }

    #[test]
    fn canceled_request_cannot_complete_a_new_generation() {
        let mut s = unlimited(0, 0);
        let old = s.update(SkyrimPos::default()).requests[0];
        assert!(s.mark_loading(old));
        s.mark_unloaded(old.cell);
        let new = s.update(SkyrimPos::default()).requests[0];
        assert_ne!(old.generation, new.generation);
        assert!(s.mark_loading(new));
        assert!(!s.mark_completed(old));
        assert!(!s.mark_failed(old));
        assert_eq!(s.state(new.cell), Some(CellState::Loading));
        assert_eq!(s.resident_count(), 0);
    }

    #[test]
    fn failures_retry_after_backoff_and_stop_at_three_attempts() {
        let mut s = unlimited(0, 0);
        let first = s.update(SkyrimPos::default()).requests[0];
        assert!(s.mark_failed(first));
        assert!(s.update(SkyrimPos::default()).load.is_empty());
        let second = s.update(SkyrimPos::default()).requests[0];
        assert_ne!(first.generation, second.generation);
        assert!(s.mark_failed(second));
        for _ in 0..3 {
            assert!(s.update(SkyrimPos::default()).load.is_empty());
        }
        let third = s.update(SkyrimPos::default()).requests[0];
        assert!(s.mark_failed(third));
        for _ in 0..32 {
            assert!(s.update(SkyrimPos::default()).load.is_empty());
        }
        assert_eq!(s.state(third.cell), Some(CellState::Failed));
        s.mark_unloaded(third.cell);
        assert_eq!(s.update(SkyrimPos::default()).requests.len(), 1);
    }

    #[test]
    fn old_coverage_stays_until_replacement_is_drawable() {
        let mut s = unlimited(0, 0);
        settle_drawable(&mut s, SkyrimPos::default());
        let focus = SkyrimPos::new(2.0 * CELL_UNITS, 0.0, 0.0);
        let plan = s.update(focus);
        assert!(plan.unload.is_empty());
        for request in &plan.requests {
            assert!(s.mark_loading(*request));
            assert!(s.mark_completed(*request));
        }
        assert!(s.update(focus).unload.is_empty());
        assert!(s.is_resident(CellCoord::new(0, 0)));
        for request in plan.requests {
            assert!(s.mark_drawable(request));
        }
        let retirement = s.update(focus);
        assert_eq!(retirement.unload, vec![CellCoord::new(0, 0)]);
        assert!(s.is_resident(CellCoord::new(0, 0)));
        acknowledge(&mut s, &retirement);
        assert!(!s.is_resident(CellCoord::new(0, 0)));
    }

    #[test]
    fn revived_retirement_rejects_old_and_reissued_unload_responses() {
        let mut s = unlimited(0, 0);
        settle_drawable(&mut s, SkyrimPos::default());
        let far = SkyrimPos::new(8.0 * CELL_UNITS, 0.0, 0.0);
        let plan = s.update(far);
        acknowledge(&mut s, &plan);
        let old = s.update(far).retirements[0];
        s.update(SkyrimPos::default());
        assert!(!s.mark_retired(old));
        assert!(s.is_resident(old.cell));
        settle_drawable(&mut s, far);
        assert!(!s.mark_retired(old));
    }

    #[test]
    fn moving_focus_requests_the_next_cell_even_with_zero_radius() {
        let mut s = unlimited(0, 0);
        settle_drawable(&mut s, SkyrimPos::default());
        s.set_view([1.0, 0.0, 0.0]);
        let plan = s.update(SkyrimPos::new(10.0, 0.0, 0.0));
        assert_eq!(plan.load, vec![CellCoord::new(1, 0)]);
        assert_eq!(s.resident_count(), 1);
    }

    #[test]
    fn failed_budget_scans_do_not_consume_aged_service() {
        let mut s = unlimited(1, 1);
        s.set_view([1.0, 0.0, 0.0]);
        let budget = StreamBudget {
            triangles: 1,
            bytes: 1,
        };
        let plan = s.update_with_budget(SkyrimPos::default(), budget, |_| StreamCost {
            triangles: 2,
            bytes: 2,
        });
        assert!(plan.load.is_empty());
        assert_eq!(s.served, 0);
    }

    #[test]
    fn shallow_diagonal_movement_predicts_the_east_cell() {
        let mut s = unlimited(0, 0);
        settle_drawable(&mut s, SkyrimPos::new(1.0, 1.0, 0.0));
        s.set_view([1.0, 0.0, 0.0]);
        let plan = s.update(SkyrimPos::new(400.0, 2.0, 0.0));
        assert_eq!(plan.load, vec![CellCoord::new(1, 0)]);
    }

    #[test]
    fn failed_retirement_restores_drawable_and_invalidates_old_token() {
        let mut s = unlimited(0, 0);
        settle_drawable(&mut s, SkyrimPos::default());
        let far = SkyrimPos::new(8.0 * CELL_UNITS, 0.0, 0.0);
        let plan = s.update(far);
        acknowledge(&mut s, &plan);
        let token = s.update(far).retirements[0];
        assert!(s.mark_retirement_failed(token));
        assert_eq!(s.state(token.cell), Some(CellState::Drawable));
        let new = s.update(far).retirements[0];
        assert_ne!(new.generation, token.generation);
        assert!(!s.mark_retired(token));
        assert!(s.mark_retired(new));
    }
    #[test]
    fn retry_preflight_includes_exactly_the_next_due_attempt() {
        let mut s = unlimited(0, 0);
        let first = s.update(SkyrimPos::default()).requests[0];
        assert!(s.mark_failed(first));
        assert!(s.candidates(SkyrimPos::default()).is_empty());
        assert!(s.update(SkyrimPos::default()).is_empty());
        assert_eq!(s.candidates(SkyrimPos::default()), vec![first.cell]);
        assert_eq!(s.update(SkyrimPos::default()).requests.len(), 1);
    }

    #[test]
    fn queued_age_survives_camera_turns_until_eighth_admission() {
        let mut s = CellStreamer::new(StreamConfig {
            max_loads_per_tick: 1,
            ..StreamConfig::default()
        })
        .unwrap();
        for i in 0..7 {
            s.set_view(if i % 2 == 0 {
                [1.0, 0.0, 0.0]
            } else {
                [-1.0, 0.0, 0.0]
            });
            let plan = s.update(SkyrimPos::default());
            acknowledge(&mut s, &plan);
        }
        let oldest = s
            .candidates(SkyrimPos::default())
            .into_iter()
            .min_by_key(|cell| s.queued_age.get(cell).copied())
            .unwrap();
        s.set_view([0.0, -1.0, 0.0]);
        let eighth = s.update(SkyrimPos::default());
        assert_eq!(eighth.load, vec![oldest]);
    }
    #[test]
    fn recreated_planner_rejects_previous_session_responses() {
        let mut old = unlimited(0, 0);
        let request = old.update(SkyrimPos::default()).requests[0];
        let mut replacement = unlimited(0, 0);
        let current = replacement.update(SkyrimPos::default()).requests[0];
        assert_ne!(current.generation, request.generation);
        assert!(replacement.mark_loading(current));
        assert!(!replacement.mark_completed(request));
        assert!(!replacement.mark_failed(request));
        assert_eq!(replacement.state(current.cell), Some(CellState::Loading));
    }
}

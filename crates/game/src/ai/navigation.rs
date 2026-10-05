// SPDX-License-Identifier: GPL-3.0-or-later

//! Frame-sliced NAVM requests, following Detour's sliced A* query contract.
//! Request state is transient. Actors retain their current package step while waiting.
//! Rayon executes disjoint pending requests over the immutable graph, as recorded
//! in docs/threading.md. Budget reservation and result application stay ordered.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashSet};
use std::task::Poll;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use soe_navmesh::{NavGraph, NodeId, Path, PathSearch};

use super::{distance, GlobalFormId, PlannedPath, World};

const REQUEST_NODE_BUDGET: usize = 64;
const FRAME_NODE_BUDGET: usize = 512;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Goal {
    Point {
        to: [f32; 3],
        cell: Option<GlobalFormId>,
        fallback: bool,
    },
    Flee {
        threat: [f32; 3],
    },
}

type ResultPath = Option<([f32; 3], PlannedPath)>;

/// Last-frame measurements for resumed navigation jobs, excluding serial setup.
#[derive(Debug, Default, Clone)]
pub struct PlanningWorkers {
    /// Jobs completed before returning to the actor driver.
    pub jobs: usize,
    /// Elapsed request execution per pool worker, indexed by Rayon worker index.
    pub busy: Vec<Duration>,
    /// Caller time inside pool dispatch and join, including scheduling overhead.
    pub main_thread_wait: Duration,
    /// Summed worker execution per search phase. Concurrent phases can overlap.
    pub phase_busy: BTreeMap<&'static str, Duration>,
}

impl PlanningWorkers {
    /// Fraction of available worker time occupied by requests during dispatch.
    pub fn utilisation(&self) -> f64 {
        let available = self.main_thread_wait.as_secs_f64() * self.busy.len() as f64;
        if available == 0.0 {
            return 0.0;
        }
        self.busy.iter().map(Duration::as_secs_f64).sum::<f64>() / available
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Candidate {
    distance: f32,
    node: NodeId,
}

impl Eq for Candidate {}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .total_cmp(&other.distance)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Flee {
    frontier: Vec<NodeId>,
    seen: HashSet<NodeId>,
    candidates: BinaryHeap<Candidate>,
    flooded: bool,
    search: Option<PathSearch>,
    destination: [f32; 3],
}

enum Work {
    Route(Option<PathSearch>),
    Flee(Flee),
    Finished,
}

struct Request {
    from: [f32; 3],
    goal: Goal,
    revision: u64,
    work: Work,
    result: Poll<ResultPath>,
    frame: u64,
}

pub(super) struct Planner {
    requests: BTreeMap<GlobalFormId, Request>,
    frame: u64,
    remaining: usize,
    pub(super) last_work: usize,
    pub(super) phases: BTreeMap<&'static str, std::time::Duration>,
    next_actor: Option<GlobalFormId>,
    pool: Option<rayon::ThreadPool>,
    pub(super) workers: PlanningWorkers,
}

impl Default for Planner {
    fn default() -> Self {
        Self {
            requests: BTreeMap::new(),
            frame: 0,
            remaining: FRAME_NODE_BUDGET,
            last_work: 0,
            phases: BTreeMap::new(),
            next_actor: None,
            pool: None,
            workers: PlanningWorkers::default(),
        }
    }
}

impl Planner {
    pub(super) fn set_workers(
        &mut self,
        workers: std::num::NonZeroUsize,
    ) -> Result<(), rayon::ThreadPoolBuildError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers.get())
            .thread_name(|index| format!("soe-ai-{index}"))
            .build()?;
        self.pool = Some(pool);
        Ok(())
    }

    pub(super) fn clear(&mut self) {
        self.requests.clear();
    }

    pub(super) fn retain_actors(&mut self, actors: &[GlobalFormId]) {
        self.requests
            .retain(|actor, _| actors.binary_search(actor).is_ok());
    }

    pub(super) fn cancel(&mut self, actor: GlobalFormId) {
        self.requests.remove(&actor);
    }

    pub(super) fn moved(&self, actor: GlobalFormId, position: [f32; 3]) -> bool {
        self.requests
            .get(&actor)
            .is_some_and(|request| request.from != position)
    }

    pub(super) fn waiting(&self, actor: GlobalFormId) -> bool {
        self.requests
            .get(&actor)
            .is_some_and(|request| request.result.is_pending())
    }

    #[cfg(test)]
    pub(super) fn begin_frame(&mut self, graph: &NavGraph, cells: &[Option<GlobalFormId>]) {
        self.begin_frame_attributed(graph, cells, |actor| {
            soe_trace::Entity::new("actor", actor.to_string(), "<unknown>", "planning")
        });
    }

    pub(super) fn begin_frame_attributed(
        &mut self,
        graph: &NavGraph,
        cells: &[Option<GlobalFormId>],
        entity: impl Fn(GlobalFormId) -> soe_trace::Entity,
    ) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::AiPlanning);
        self.frame = self.frame.wrapping_add(1);
        self.remaining = FRAME_NODE_BUDGET;
        self.last_work = 0;
        self.phases.clear();
        self.workers = PlanningWorkers::default();
        self.requests
            .retain(|_, request| request.revision == graph.query_revision());
        let mut pending: Vec<_> = self
            .requests
            .iter_mut()
            .filter(|(_, request)| request.result.is_pending())
            .collect();
        let first = pending
            .partition_point(|(actor, _)| self.next_actor.is_some_and(|next| **actor < next));
        pending.rotate_left(first);
        if pending.is_empty() {
            return;
        }
        let entities = soe_trace::Profile::enabled().then(|| {
            pending
                .iter()
                .map(|(actor, _)| (**actor, entity(**actor)))
                .collect::<BTreeMap<_, _>>()
        });
        let recorder = soe_trace::WorkRecorder::current();
        let workers = self.pool.as_ref().map_or_else(
            rayon::current_num_threads,
            rayon::ThreadPool::current_num_threads,
        );
        self.workers.busy = vec![Duration::ZERO; workers];
        let mut offset = 0;
        while offset < pending.len() {
            if self.remaining == 0 {
                self.next_actor = Some(*pending[offset].0);
                break;
            }
            let budget = self.remaining.min(REQUEST_NODE_BUDGET);
            let count = (self.remaining / budget).min(pending.len() - offset);
            let batch = &mut pending[offset..offset + count];
            let mut execute = || {
                batch
                    .par_iter_mut()
                    .map(|(actor, request)| {
                        let _work = recorder.as_ref().map(soe_trace::WorkRecorder::enter);
                        let actor = **actor;
                        let _entity = entities.as_ref().map(|entities| {
                            soe_trace::EntityScope::new(|| entities[&actor].clone(), 1)
                        });
                        let started = Instant::now();
                        let mut phases = BTreeMap::new();
                        let used = request.advance(graph, cells, budget, &mut phases);
                        (
                            used,
                            phases,
                            started.elapsed(),
                            rayon::current_thread_index(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let started = Instant::now();
            let results = if let Some(pool) = &self.pool {
                pool.install(execute)
            } else {
                execute()
            };
            self.workers.main_thread_wait += started.elapsed();
            for ((_, request), (used, phases, busy, worker)) in batch.iter_mut().zip(results) {
                self.remaining -= used;
                self.last_work += used;
                request.frame = self.frame;
                self.workers.jobs += 1;
                if let Some(worker) = worker {
                    self.workers.busy[worker] += busy;
                }
                for (phase, elapsed) in phases {
                    *self.workers.phase_busy.entry(phase).or_default() += elapsed;
                }
            }
            offset += count;
        }
    }

    fn request(
        &mut self,
        graph: &NavGraph,
        cells: &[Option<GlobalFormId>],
        actor: GlobalFormId,
        from: [f32; 3],
        goal: Goal,
    ) -> Poll<ResultPath> {
        if self.requests.get(&actor).is_some_and(|r| {
            r.from != from || r.goal != goal || r.revision != graph.query_revision()
        }) {
            self.requests.remove(&actor);
        }
        let setup_started = std::time::Instant::now();
        let request = self.requests.entry(actor).or_insert_with(|| {
            let work = match goal {
                Goal::Point { to, .. } => Work::Route(graph.start_path_between(from, to)),
                Goal::Flee { .. } => {
                    let start = graph.locate(from).or_else(|| graph.nearest(from));
                    Work::Flee(Flee {
                        frontier: start.into_iter().collect(),
                        seen: HashSet::new(),
                        candidates: BinaryHeap::new(),
                        flooded: false,
                        search: None,
                        destination: from,
                    })
                }
            };
            Request {
                from,
                goal,
                revision: graph.query_revision(),
                work,
                result: Poll::Pending,
                frame: self.frame.wrapping_sub(1),
            }
        });
        *self.phases.entry("request_setup").or_default() += setup_started.elapsed();
        if request.result.is_pending() && request.frame != self.frame {
            let used = request.advance(
                graph,
                cells,
                self.remaining.min(REQUEST_NODE_BUDGET),
                &mut self.phases,
            );
            self.remaining -= used;
            self.last_work += used;
            request.frame = self.frame;
        }
        if matches!(&request.result, Poll::Ready(None))
            || matches!(&request.result, Poll::Ready(Some((_, (path, _)))) if path.is_empty())
        {
            request.work = Work::Finished;
            request.result.clone()
        } else if request.result.is_ready() {
            self.requests
                .remove(&actor)
                .map_or(Poll::Ready(None), |r| r.result)
        } else {
            Poll::Pending
        }
    }
}

fn planned(
    path: Path,
    cells: &[Option<GlobalFormId>],
    fallback: Option<GlobalFormId>,
) -> PlannedPath {
    let owners = path
        .point_nodes
        .iter()
        .skip(1)
        .map(|node| {
            cells
                .get(node.mesh as usize)
                .copied()
                .flatten()
                .or(fallback)
        })
        .collect();
    (path.points.into_iter().skip(1).collect(), owners)
}

impl Request {
    fn advance(
        &mut self,
        graph: &NavGraph,
        cells: &[Option<GlobalFormId>],
        budget: usize,
        phases: &mut BTreeMap<&'static str, std::time::Duration>,
    ) -> usize {
        if budget == 0 {
            return 0;
        }
        match &mut self.work {
            Work::Finished => 0,
            Work::Route(search) => {
                let Goal::Point { to, cell, fallback } = self.goal else {
                    return 0;
                };
                let (result, used) = if let Some(search) = search {
                    let started = std::time::Instant::now();
                    let result = search.advance(graph, budget);
                    *phases.entry("search_advance").or_default() += started.elapsed();
                    (result, search.last_work())
                } else {
                    (Poll::Ready(None), 1)
                };
                if let Poll::Ready(path) = result {
                    let started = std::time::Instant::now();
                    let path = path
                        .map(|path| planned(path, cells, if fallback { None } else { cell }))
                        .or_else(|| {
                            fallback.then(|| {
                                if graph.contains_point(self.from) && graph.contains_point(to) {
                                    (Vec::new(), Vec::new())
                                } else {
                                    (vec![to], vec![cell])
                                }
                            })
                        });
                    *phases.entry("route_result").or_default() += started.elapsed();
                    self.result = Poll::Ready(path.map(|path| (to, path)));
                }
                used
            }
            Work::Flee(flee) => {
                let Goal::Flee { threat } = self.goal else {
                    return 0;
                };
                let started = std::time::Instant::now();
                let mut used = 0;
                while used < budget {
                    if !flee.flooded {
                        if let Some(node) = flee.frontier.pop() {
                            used += 1;
                            if !flee.seen.insert(node) || !graph.is_walkable(node) {
                                continue;
                            }
                            let point = graph.centroid(node);
                            let d = distance(point, threat);
                            if d > distance(self.from, threat) + f32::EPSILON {
                                flee.candidates.push(Candidate { distance: d, node });
                            }
                            flee.frontier
                                .extend(graph.triangle(node).neighbors.iter().flatten().copied());
                            continue;
                        }
                        flee.flooded = true;
                    }
                    if let Some(search) = &mut flee.search {
                        let result = search.advance(graph, budget - used);
                        used += search.last_work();
                        match result {
                            Poll::Pending => break,
                            Poll::Ready(Some(path)) => {
                                self.result = Poll::Ready(Some((
                                    flee.destination,
                                    planned(path, cells, None),
                                )));
                                break;
                            }
                            Poll::Ready(None) => flee.search = None,
                        }
                    } else if let Some(candidate) = flee.candidates.pop() {
                        used += 1;
                        flee.destination = graph.centroid(candidate.node);
                        flee.search = graph.start_path_between(self.from, flee.destination);
                    } else {
                        self.result = Poll::Ready(None);
                        break;
                    }
                }
                *phases.entry("flee_search").or_default() += started.elapsed();
                used
            }
        }
    }
}

pub(super) fn request(
    world: &mut World,
    actor: GlobalFormId,
    from: [f32; 3],
    goal: Goal,
) -> Poll<ResultPath> {
    super::measure_phase(world, "path_requests", |world| {
        super::ensure_nav_graph(world);
        let Some(graph) = &world.ai.nav else {
            return Poll::Ready(None);
        };
        world
            .ai
            .planner
            .request(graph, &world.ai.nav_cells, actor, from, goal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_child(name: &str) -> bool {
        if std::env::var("SOE_PRF_13_CHILD").is_ok_and(|value| value == "1") {
            return false;
        }
        let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env("SOE_PRF_13_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        true
    }

    #[test]
    fn prf_13_resumed_navigation_charges_each_requesting_actor() {
        if profile_child("prf_13_resumed_navigation_charges_each_requesting_actor") {
            return;
        }
        use soe_trace::Profile;
        Profile::disable();
        let graph = strip();
        let mut planner = Planner::default();
        let actors = [actor(0x800), actor(0x801)];
        for id in actors {
            assert!(planner
                .request(&graph, &[None], id, [10.0, 25.0, 0.0], goal())
                .is_pending());
        }
        Profile::clear();
        Profile::enable();
        planner.begin_frame_attributed(&graph, &[None], profile_actor);
        Profile::disable();
        let rows = Profile::summary();
        for actor in actors {
            let (_, stats) = rows
                .iter()
                .find(|(e, _)| {
                    e.kind == "actor"
                        && e.id == actor.to_string()
                        && e.phase == "planning"
                        && e.plugin == "Prf11.esm"
                })
                .unwrap();
            assert_eq!(stats.calls, 1);
            assert!(stats.total_ns > 0);
        }
    }

    fn profile_actor(id: GlobalFormId) -> soe_trace::Entity {
        soe_trace::Entity::new("actor", id.to_string(), "Prf11.esm", "planning")
    }

    fn strip() -> NavGraph {
        let mut graph = NavGraph::new();
        let mut vertices = Vec::new();
        for x in 0..=200 {
            vertices.push([x as f32 * 100.0, 0.0, 0.0]);
            vertices.push([x as f32 * 100.0, 100.0, 0.0]);
        }
        let mut triangles = Vec::new();
        for x in 0..200 {
            let a = x * 2;
            triangles.push([a, a + 2, a + 3]);
            triangles.push([a, a + 3, a + 1]);
        }
        graph.add_mesh_with_adjacency(vertices, &triangles);
        graph.prepare_queries();
        graph
    }

    fn actor(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn goal() -> Goal {
        Goal::Point {
            to: [19990.0, 25.0, 0.0],
            cell: None,
            fallback: true,
        }
    }

    fn serial_frame(planner: &mut Planner, graph: &NavGraph, cells: &[Option<GlobalFormId>]) {
        planner.frame = planner.frame.wrapping_add(1);
        planner.remaining = FRAME_NODE_BUDGET;
        planner.last_work = 0;
        planner.phases.clear();
        planner
            .requests
            .retain(|_, request| request.revision == graph.query_revision());
        let keys: Vec<_> = planner.requests.keys().copied().collect();
        let start = planner
            .next_actor
            .map_or(0, |next| keys.partition_point(|key| *key < next));
        for offset in 0..keys.len() {
            let actor = keys[(start + offset) % keys.len()];
            let request = planner.requests.get_mut(&actor).unwrap();
            if !request.result.is_pending() {
                continue;
            }
            if planner.remaining == 0 {
                planner.next_actor = Some(actor);
                break;
            }
            let used = request.advance(
                graph,
                cells,
                planner.remaining.min(REQUEST_NODE_BUDGET),
                &mut planner.phases,
            );
            planner.remaining -= used;
            planner.last_work += used;
            request.frame = planner.frame;
        }
    }

    fn pending_planner(graph: &NavGraph, workers: usize) -> Planner {
        let mut planner = Planner::default();
        planner.set_workers(workers.try_into().unwrap()).unwrap();
        for id in 0..32 {
            let goal = if id % 5 == 0 {
                Goal::Flee {
                    threat: [0.0, 25.0, 0.0],
                }
            } else {
                Goal::Point {
                    to: [500.0 + ((id * 7919) % 19000) as f32, 25.0, 0.0],
                    cell: Some(actor(0x800)),
                    fallback: true,
                }
            };
            let _ = planner.request(
                graph,
                &[Some(actor(0x800))],
                actor(id),
                [10.0, 25.0, 0.0],
                goal,
            );
        }
        planner
    }

    #[test]
    fn prf_17_navigation_matches_serial_order_budgets_and_work_with_one_and_eight_workers() {
        use soe_trace::WorkFrame;

        let graph = strip();
        let cells = [Some(actor(0x800))];
        let mut serial = pending_planner(&graph, 1);
        let mut single = pending_planner(&graph, 1);
        let mut parallel = pending_planner(&graph, 8);
        for _ in 0..256 {
            let frame = WorkFrame::begin();
            serial_frame(&mut serial, &graph, &cells);
            let expected_counts = frame.counts();
            drop(frame);
            for planner in [&mut single, &mut parallel] {
                let frame = WorkFrame::begin();
                planner.begin_frame(&graph, &cells);
                assert_eq!(frame.counts(), expected_counts);
                assert_eq!(planner.last_work, serial.last_work);
                assert!(planner.last_work <= FRAME_NODE_BUDGET);
                assert_eq!(planner.remaining, serial.remaining);
                assert_eq!(planner.next_actor, serial.next_actor);
                for (id, expected) in &serial.requests {
                    let actual = &planner.requests[id];
                    assert_eq!(actual.frame, expected.frame, "actor {id}");
                    assert_eq!(actual.result, expected.result, "actor {id}");
                }
                assert!(planner.workers.utilisation() <= 1.0);
            }
            if serial
                .requests
                .values()
                .all(|request| request.result.is_ready())
            {
                assert!(single.workers.jobs > 0);
                assert!(parallel.workers.jobs > 0);
                assert_eq!(single.workers.busy.len(), 1);
                assert_eq!(parallel.workers.busy.len(), 8);
                assert!(parallel.workers.busy.iter().any(|time| !time.is_zero()));
                return;
            }
        }
        panic!("navigation requests starved for 256 frames");
    }

    #[test]
    fn prf_17_navigation_cancellation_revision_and_idle_metrics() {
        for workers in [1, 8] {
            let mut graph = strip();
            let mut planner = pending_planner(&graph, workers);
            planner.begin_frame(&graph, &[None]);
            assert!(planner.workers.jobs > 0);
            planner.cancel(actor(3));
            planner.retain_actors(&[actor(0), actor(3)]);
            assert_eq!(
                planner.requests.keys().copied().collect::<Vec<_>>(),
                [actor(0)]
            );
            graph.add_mesh_with_adjacency(
                vec![[0.0, 200.0, 0.0], [100.0, 200.0, 0.0], [0.0, 300.0, 0.0]],
                &[[0, 1, 2]],
            );
            graph.prepare_queries();
            planner.begin_frame(&graph, &[None; 2]);
            assert!(planner.requests.is_empty());
            assert_eq!(planner.workers.jobs, 0);
            assert_eq!(planner.workers.main_thread_wait, Duration::ZERO);
            assert_eq!(planner.workers.utilisation(), 0.0);
        }
    }

    #[test]
    fn prf_17_navigation_reports_worker_utilisation_and_join_time() {
        let graph = strip();
        for workers in [1, 8] {
            let mut planner = pending_planner(&graph, workers);
            let snapshot = soe_trace::FrameSnapshot::now();
            let started = Instant::now();
            let mut wait = Duration::ZERO;
            let mut busy = vec![Duration::ZERO; workers];
            let mut frames = 0;
            while planner
                .requests
                .values()
                .any(|request| request.result.is_pending())
            {
                planner.begin_frame(&graph, &[None]);
                assert!(planner.last_work <= FRAME_NODE_BUDGET);
                wait += planner.workers.main_thread_wait;
                for (total, frame) in busy.iter_mut().zip(&planner.workers.busy) {
                    *total += *frame;
                }
                frames += 1;
                assert!(frames < 256);
            }
            assert!(wait > Duration::ZERO);
            let sample = snapshot.finish();
            assert!(
                u128::from(sample.system_ns[soe_trace::System::AiPlanning as usize])
                    >= wait.as_nanos()
            );
            let utilisation = busy.iter().map(Duration::as_secs_f64).sum::<f64>()
                / (wait.as_secs_f64() * workers as f64);
            eprintln!(
                "PRF-17 navigation workers={workers} frames={frames} wall={:?} main_wait={wait:?} busy={busy:?} utilisation={utilisation:.3}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn prf_11_marker_filter_keeps_unrelated_references_uninitialized() {
        use crate::ai::{idle_markers, sandbox_markers};
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

        let mut plugin = Plugin::new(FileHeader::default());
        for (tag, object) in [
            ("CELL", 0x800),
            ("NPC_", 0x801),
            ("STAT", 0x802),
            ("IDLM", 0x803),
        ] {
            plugin.push_top(Record::new(Tag(tag.as_bytes().try_into().unwrap()), object));
        }
        for (tag, object, base) in [
            ("ACHR", 0x810, 0x801u32),
            ("REFR", 0x811, 0x802),
            ("REFR", 0x812, 0x802),
        ] {
            let mut record = Record::new(Tag(tag.as_bytes().try_into().unwrap()), object)
                .sub(Tag(*b"NAME"), base.to_le_bytes());
            record.placement.cell = Some(0x800);
            plugin.push_top(record);
        }
        let mut order = LoadOrder::new();
        order.add("Prf11Markers.esm", plugin).unwrap();
        let mut world = World::new(order);
        world
            .mod_data
            .reference_swaps
            .insert(actor(0x812), actor(0x803));
        assert_eq!(
            sandbox_markers(&mut world, actor(0x800), [0.0; 3], 10.0),
            vec![actor(0x812)]
        );
        assert_eq!(
            idle_markers(&mut world, actor(0x810), actor(0x800), [0.0; 3], 10.0),
            vec![actor(0x812)]
        );
        assert!(world.reference_ref(actor(0x810)).is_none());
        assert!(world.reference_ref(actor(0x811)).is_none());
        assert_eq!(world.loaded_actors(), []);
        world.reference(actor(0x812)).unwrap().base = Some(actor(0x802));
        assert_eq!(
            sandbox_markers(&mut world, actor(0x800), [0.0; 3], 10.0),
            []
        );
        assert_eq!(
            idle_markers(&mut world, actor(0x810), actor(0x800), [0.0; 3], 10.0),
            []
        );
    }

    #[test]
    fn prf_11_phase_timings_reset_and_cover_early_returns() {
        use crate::ai::{select, tick};
        use soe_esm::LoadOrder;
        let mut world = World::new(LoadOrder::new());
        world
            .ai
            .planning_phases
            .insert("stale", std::time::Duration::MAX);
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        tick(&mut world, 0.1, &mut alloc);
        assert!(!world.ai.planning_phases.contains_key("stale"));
        for phase in ["scheduling", "navigation_resume"] {
            assert!(world.ai.planning_phases[phase] <= world.ai.planning_time);
        }
        assert!(select(&mut world, actor(0x800)).is_none());
        assert!(world.ai.planning_phases.contains_key("package_selection"));
        tick(&mut world, 0.1, &mut alloc);
        assert!(!world.ai.planning_phases.contains_key("package_selection"));
    }

    #[test]
    fn prf_11_cell_loading_prepares_navigation_before_the_first_tick() {
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
        let mut order = LoadOrder::new();
        order.add("Prf11CellLoad.esm", plugin).unwrap();
        let mut world = World::new(order);
        assert!(world.ai.nav_graph().is_none());
        world.load_cell(actor(0x800));
        assert!(world.ai.nav_graph().is_some());
    }

    #[test]
    fn prf_11_actor_driver_timings_cover_live_and_dead_actors() {
        use crate::ai::tick;
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
        plugin
            .push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
        let mut order = LoadOrder::new();
        order.add("Prf11Driver.esm", plugin).unwrap();
        let mut world = World::new(order);
        let id = actor(0x801);
        world.actor(id).unwrap();
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        tick(&mut world, 0.1, &mut alloc);
        for phase in [
            "actor_state",
            "calm_check",
            "evaluation_due",
            "combat_target",
            "combat_acquisition",
            "movement",
        ] {
            assert!(
                world.ai.planning_phases[phase] <= world.ai.planning_phases["actor_driver"],
                "{phase} exceeded actor processing"
            );
        }
        world.actor(id).unwrap().dead = true;
        tick(&mut world, 0.1, &mut alloc);
        assert!(world.ai.planning_phases.contains_key("actor_state"));
        assert!(!world.ai.planning_phases.contains_key("movement"));
    }

    #[test]
    fn prf_11_pending_actor_retains_package_step_and_resumes_movement() {
        use crate::ai::{step, AiState, Procedure};
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
        plugin
            .push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
        let mut order = LoadOrder::new();
        order.add("Prf11.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.ai.nav = Some(strip());
        world.ai.nav_cells = vec![None];
        let id = actor(0x801);
        world.reference(id).unwrap().position = [10.0, 25.0, 0.0];
        let before = AiState {
            procedure: Procedure::Travel,
            destination: Some([19990.0, 25.0, 0.0]),
            speed: 100.0,
            ..AiState::default()
        };
        world.actor(id).unwrap().ai = before.clone();
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        step(&mut world, id, 0.1, &mut alloc);
        assert!(world.ai.planner.waiting(id));
        assert_eq!(world.actor(id).unwrap().ai, before);
        assert_eq!(world.reference(id).unwrap().position, [10.0, 25.0, 0.0]);
        let mut frames = 0;
        while world.ai.planner.waiting(id) {
            world.ai.planner.begin_frame_attributed(
                world.ai.nav.as_ref().unwrap(),
                &world.ai.nav_cells,
                |id| soe_trace::Entity::new("actor", id.to_string(), "Prf11.esm", "planning"),
            );
            frames += 1;
            assert!(world.ai.planner.last_work <= REQUEST_NODE_BUDGET);
            assert_eq!(world.actor(id).unwrap().ai, before);
            assert!(frames < 100);
        }
        step(&mut world, id, 0.1, &mut alloc);
        assert!(world.reference(id).unwrap().position[0] > 10.0);
        assert_eq!(world.actor(id).unwrap().ai.procedure, Procedure::Travel);
    }

    #[test]
    fn prf_11_automatic_evaluations_are_bounded_and_keep_previous_steps() {
        use crate::ai::{tick, AiState, Procedure, FRAME_EVALUATION_BUDGET};
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
        for object in 0x801..0x815 {
            plugin.push_top(
                Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()),
            );
        }
        let mut order = LoadOrder::new();
        order.add("Prf11Evaluation.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.ai.nav = Some(strip());
        world.ai.nav_cells = vec![None];
        for object in 0x801..0x815 {
            let id = actor(object);
            world.reference(id).unwrap().position = [10.0, 25.0, 0.0];
            world.actor(id).unwrap().ai = AiState {
                procedure: Procedure::Travel,
                destination: Some([19990.0, 25.0, 0.0]),
                path: vec![[19990.0, 25.0, 0.0]],
                path_cells: vec![None],
                speed: 100.0,
                reevaluate: true,
                ..AiState::default()
            };
        }
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        for frame in 0..5 {
            world.clock.real_seconds = f64::from(frame + 1) * 0.1;
            let before = world.ai.evaluations;
            tick(&mut world, 0.1, &mut alloc);
            assert!(world.ai.evaluations - before <= FRAME_EVALUATION_BUDGET);
            for object in 0x801..0x815 {
                let id = actor(object);
                assert_eq!(
                    world.reference(id).unwrap().position[0],
                    20.0 + frame as f32 * 10.0
                );
                assert_eq!(world.actor(id).unwrap().ai.procedure, Procedure::Travel);
            }
        }
        assert!(
            world
                .loaded_actors()
                .iter()
                .all(|id| !world.actor_ref(*id).unwrap().ai.reevaluate),
            "every actor received an evaluation slot"
        );
    }

    #[test]
    fn prf_11_request_and_frame_budgets_hold_without_starvation() {
        let graph = strip();
        let mut planner = Planner::default();
        for id in 0..20 {
            assert!(planner
                .request(&graph, &[None], actor(id), [10.0, 25.0, 0.0], goal())
                .is_pending());
        }
        assert_eq!(planner.last_work, FRAME_NODE_BUDGET);
        let mut completed = HashSet::new();
        for _ in 0..100 {
            planner.begin_frame_attributed(&graph, &[None], profile_actor);
            assert!(planner.last_work <= FRAME_NODE_BUDGET);
            for request in planner.requests.values() {
                if let Work::Route(Some(search)) = &request.work {
                    assert!(search.last_work() <= REQUEST_NODE_BUDGET);
                }
            }
            for id in 0..20 {
                if !completed.contains(&id) && !planner.waiting(actor(id)) {
                    assert!(matches!(
                        planner.request(&graph, &[None], actor(id), [10.0, 25.0, 0.0], goal()),
                        Poll::Ready(Some(_))
                    ));
                    completed.insert(id);
                }
            }
            if completed.len() == 20 {
                break;
            }
        }
        assert_eq!(completed.len(), 20);
    }

    #[test]
    fn prf_11_flee_flood_resumes_and_selects_the_original_furthest_point() {
        let graph = strip();
        let from = [10.0, 25.0, 0.0];
        let threat = [0.0, 25.0, 0.0];
        let expected = graph
            .reachable(graph.locate(from).unwrap())
            .into_iter()
            .max_by(|a, b| {
                distance(graph.centroid(*a), threat)
                    .total_cmp(&distance(graph.centroid(*b), threat))
                    .then_with(|| b.cmp(a))
            })
            .unwrap();
        let mut planner = Planner::default();
        let goal = Goal::Flee { threat };
        assert!(planner
            .request(&graph, &[None], actor(1), from, goal)
            .is_pending());
        for _ in 0..100 {
            planner.begin_frame_attributed(&graph, &[None], profile_actor);
            assert!(planner.last_work <= REQUEST_NODE_BUDGET);
            if !planner.waiting(actor(1)) {
                break;
            }
        }
        let Poll::Ready(Some((destination, path))) =
            planner.request(&graph, &[None], actor(1), from, goal)
        else {
            panic!("flee request did not complete")
        };
        assert_eq!(destination, graph.centroid(expected));
        assert_eq!(
            path.0,
            graph.find_path_between(from, destination).unwrap().points[1..]
        );
    }

    #[test]
    fn prf_11_failed_routes_are_reused_until_graph_or_request_changes() {
        let mut graph = strip();
        let island = graph.add_mesh_with_adjacency(
            vec![[0.0, 200.0, 0.0], [100.0, 200.0, 0.0], [0.0, 300.0, 0.0]],
            &[[0, 1, 2]],
        );
        let from = [10.0, 25.0, 0.0];
        let to = [10.0, 210.0, 0.0];
        for fallback in [false, true] {
            let mut planner = Planner::default();
            let goal = Goal::Point {
                to,
                cell: None,
                fallback,
            };
            let expected = if fallback {
                Poll::Ready(Some((to, (Vec::new(), Vec::new()))))
            } else {
                Poll::Ready(None)
            };
            assert_eq!(
                planner.request(&graph, &[None; 2], actor(1), from, goal),
                expected
            );
            assert_eq!(planner.last_work, 1);
            for _ in 0..3600 {
                planner.begin_frame_attributed(&graph, &[None; 2], profile_actor);
                assert_eq!(
                    planner.request(&graph, &[None; 2], actor(1), from, goal),
                    expected
                );
                assert_eq!(
                    planner.last_work, 0,
                    "unchanged failed route was searched again"
                );
            }
            planner.cancel(actor(1));
            assert_eq!(
                planner.request(&graph, &[None; 2], actor(1), from, goal),
                expected
            );
            assert_eq!(planner.last_work, 1, "explicit cancellation must retry");
            planner.begin_frame_attributed(&graph, &[None; 2], profile_actor);
            assert!(planner.moved(actor(1), [20.0, 25.0, 0.0]));
            assert_eq!(
                planner.request(&graph, &[None; 2], actor(1), [20.0, 25.0, 0.0], goal),
                expected
            );
            assert_eq!(planner.last_work, 1, "changed start must retry");
            planner.clear();
            planner.begin_frame_attributed(&graph, &[None; 2], profile_actor);
            assert_eq!(
                planner.request(&graph, &[None; 2], actor(1), from, goal),
                expected
            );
            assert_eq!(planner.last_work, 1, "navigation invalidation must retry");
            planner.retain_actors(&[]);
            assert!(
                planner.requests.is_empty(),
                "unloaded actor kept a failed route"
            );
        }
        let mut planner = Planner::default();
        let goal = Goal::Point {
            to,
            cell: None,
            fallback: false,
        };
        assert_eq!(
            planner.request(&graph, &[None; 2], actor(1), from, goal),
            Poll::Ready(None)
        );
        graph.link(graph.locate(from).unwrap(), 0, NodeId::new(island, 0));
        let expected = graph.find_path_between(from, to).unwrap();
        assert_eq!(
            planner.request(&graph, &[None; 2], actor(1), from, goal),
            Poll::Ready(Some((to, planned(expected, &[None; 2], None))))
        );
    }

    #[test]
    fn prf_11_changed_goal_cancels_pending_route_and_keeps_failure_policy() {
        let mut graph = strip();
        let mut planner = Planner::default();
        let from = [10.0, 25.0, 0.0];
        assert!(planner
            .request(&graph, &[None], actor(1), from, goal())
            .is_pending());
        let close = Goal::Point {
            to: [20.0, 25.0, 0.0],
            cell: None,
            fallback: true,
        };
        assert!(matches!(
            planner.request(&graph, &[None], actor(1), from, close),
            Poll::Ready(Some(_))
        ));
        graph.add_mesh_with_adjacency(
            vec![[0.0, 200.0, 0.0], [100.0, 200.0, 0.0], [0.0, 300.0, 0.0]],
            &[[0, 1, 2]],
        );
        let unreachable = Goal::Point {
            to: [10.0, 210.0, 0.0],
            cell: None,
            fallback: true,
        };
        assert_eq!(
            planner.request(&graph, &[None], actor(1), from, unreachable),
            Poll::Ready(Some(([10.0, 210.0, 0.0], (Vec::new(), Vec::new()))))
        );
        let off_mesh = Goal::Point {
            to: [10.0, 400.0, 0.0],
            cell: None,
            fallback: true,
        };
        assert_eq!(
            planner.request(&graph, &[None], actor(1), from, off_mesh),
            Poll::Ready(Some((
                [10.0, 400.0, 0.0],
                (vec![[10.0, 400.0, 0.0]], vec![None])
            )))
        );
    }
}

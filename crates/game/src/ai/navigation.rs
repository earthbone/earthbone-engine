// SPDX-License-Identifier: GPL-3.0-or-later

//! Frame-sliced NAVM requests, following Detour's sliced A* query contract.
//! Request state is transient. Actors retain their current package step while waiting.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashSet};
use std::task::Poll;

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
}

struct Request {
    from: [f32; 3],
    goal: Goal,
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
        }
    }
}

impl Planner {
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

    pub(super) fn begin_frame(&mut self, graph: &NavGraph, cells: &[Option<GlobalFormId>]) {
        self.frame = self.frame.wrapping_add(1);
        self.remaining = FRAME_NODE_BUDGET;
        self.last_work = 0;
        self.phases.clear();
        let keys: Vec<_> = self.requests.keys().copied().collect();
        let start = self
            .next_actor
            .map_or(0, |next| keys.partition_point(|key| *key < next));
        let len = keys.len();
        for offset in 0..len {
            let actor = keys[(start + offset) % len];
            let Some(request) = self.requests.get_mut(&actor) else {
                continue;
            };
            if request.result.is_pending() {
                if self.remaining == 0 {
                    self.next_actor = Some(actor);
                    break;
                }
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
        if self
            .requests
            .get(&actor)
            .is_some_and(|r| r.from != from || r.goal != goal)
        {
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
        if request.result.is_ready() {
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
        assert!(world.loaded_actors().is_empty());
        world.reference(actor(0x812)).unwrap().base = Some(actor(0x802));
        assert!(sandbox_markers(&mut world, actor(0x800), [0.0; 3], 10.0).is_empty());
        assert!(idle_markers(&mut world, actor(0x810), actor(0x800), [0.0; 3], 10.0).is_empty());
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
            world
                .ai
                .planner
                .begin_frame(world.ai.nav.as_ref().unwrap(), &world.ai.nav_cells);
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
            planner.begin_frame(&graph, &[None]);
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
            planner.begin_frame(&graph, &[None]);
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

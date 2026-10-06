// SPDX-License-Identifier: GPL-3.0-or-later

//! Frame-sliced sandbox candidate queries, using Detour's retained sliced-work
//! contract over the existing winning cell-reference index. Matches remain FormIDs.

use super::metrics::Phase;

use std::collections::BTreeMap;
use std::task::Poll;

use super::{sandbox_marker, GlobalFormId, World};

const REQUEST_BUDGET: usize = 64;
const FRAME_BUDGET: usize = 64;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Goal {
    pub(super) package: Option<GlobalFormId>,
    pub(super) cell: GlobalFormId,
    pub(super) center: [f32; 3],
    pub(super) radius: f32,
}

struct Request {
    goal: Goal,
    offsets: [usize; 3],
    total: usize,
    cursor: usize,
    matches: Vec<GlobalFormId>,
    frame: u64,
}

#[derive(Default)]
pub(super) struct Planner {
    requests: BTreeMap<GlobalFormId, Request>,
    frame: u64,
    remaining: usize,
    next_actor: Option<GlobalFormId>,
    last_work: usize,
}

impl Planner {
    pub(super) fn cancel(&mut self, actor: GlobalFormId) {
        self.requests.remove(&actor);
    }

    pub(super) fn waiting(&self, actor: GlobalFormId) -> bool {
        self.requests
            .get(&actor)
            .is_some_and(|request| request.cursor < request.total)
    }

    pub(super) fn begin_frame(&mut self, world: &mut World, actors: &[GlobalFormId]) {
        self.frame = self.frame.wrapping_add(1);
        self.remaining = FRAME_BUDGET;
        self.last_work = 0;
        self.requests
            .retain(|actor, _| actors.binary_search(actor).is_ok());
        let keys: Vec<_> = self.requests.keys().copied().collect();
        let start = self
            .next_actor
            .map_or(0, |next| keys.partition_point(|actor| *actor < next));
        for offset in 0..keys.len() {
            let actor = keys[(start + offset) % keys.len()];
            if self.remaining == 0 {
                self.next_actor = Some(actor);
                break;
            }
            if let Some(request) = self.requests.get_mut(&actor) {
                let _entity = soe_trace::EntityScope::new(
                    || world.profile_entity("actor", actor, "planning"),
                    1,
                );
                let used = request.advance(world, self.remaining.min(REQUEST_BUDGET));
                self.remaining -= used;
                self.last_work += used;
                request.frame = self.frame;
            }
        }
    }

    fn request(
        &mut self,
        world: &mut World,
        actor: GlobalFormId,
        goal: Goal,
    ) -> Poll<Vec<GlobalFormId>> {
        if self
            .requests
            .get(&actor)
            .is_some_and(|request| request.goal != goal)
        {
            self.cancel(actor);
        }
        let request = self.requests.entry(actor).or_insert_with(|| Request {
            goal,
            offsets: [0; 3],
            total: super::measure_phase(world, Phase::CellReferences, |world| {
                let index = world.order().build_cell_index();
                index.references(goal.cell).len() + index.deleted_references(goal.cell).len()
            }),
            cursor: 0,
            matches: Vec::new(),
            frame: self.frame.wrapping_sub(1),
        });
        if request.frame != self.frame {
            let used = request.advance(world, self.remaining.min(REQUEST_BUDGET));
            self.remaining -= used;
            self.last_work += used;
            request.frame = self.frame;
        }
        if request.cursor == request.total {
            Poll::Ready(
                self.requests
                    .remove(&actor)
                    .map_or_else(Vec::new, |request| request.matches),
            )
        } else {
            Poll::Pending
        }
    }
}

impl Request {
    fn advance(&mut self, world: &mut World, budget: usize) -> usize {
        let references = super::measure_phase(world, Phase::CellReferences, |world| {
            let index = world.order().build_cell_index();
            let slices = [
                index.persistent_references(self.goal.cell),
                index.temporary_references(self.goal.cell),
                index.deleted_references(self.goal.cell),
            ];
            let mut references = Vec::with_capacity(budget.min(self.total - self.cursor));
            for _ in 0..budget {
                let Some((part, reference)) = slices
                    .iter()
                    .enumerate()
                    .filter_map(|(part, slice)| slice.get(self.offsets[part]).map(|id| (part, *id)))
                    .min_by_key(|(_, id)| *id)
                else {
                    break;
                };
                self.offsets[part] += 1;
                self.cursor += 1;
                references.push(reference);
            }
            references
        });
        let used = references.len();
        for reference in references {
            if world
                .tag_of(reference)
                .is_some_and(|tag| tag == soe_esm::tag::REFR || tag == soe_esm::tag::ACHR)
                && sandbox_marker(world, reference, self.goal.center, self.goal.radius)
            {
                self.matches.push(reference);
            }
        }
        used
    }
}

pub(super) fn request(
    world: &mut World,
    actor: GlobalFormId,
    goal: Goal,
) -> Poll<Vec<GlobalFormId>> {
    super::measure_phase(world, Phase::SandboxMarkers, |world| {
        let mut planner = std::mem::take(&mut world.ai.markers);
        let result = planner.request(world, actor, goal);
        world.ai.markers = planner;
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn fixture() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        for (tag, object) in [
            (Tag(*b"CELL"), 0x800),
            (Tag(*b"IDLM"), 0x801),
            (Tag(*b"NPC_"), 0x802),
        ] {
            plugin.push_top(Record::new(tag, object));
        }
        for object in 0x1000..0x1190 {
            let mut record =
                Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x801u32.to_le_bytes());
            record.placement.cell = Some(0x800);
            plugin.push_top(record);
        }
        let mut actor =
            Record::new(Tag(*b"ACHR"), 0x810).sub(Tag(*b"NAME"), 0x802u32.to_le_bytes());
        actor.placement.cell = Some(0x800);
        plugin.push_top(actor);
        let mut order = LoadOrder::new();
        order.add("Prf11MarkerSlices.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.actor(id(0x810)).unwrap();
        world
    }

    fn goal() -> Goal {
        Goal {
            package: None,
            cell: id(0x800),
            center: [0.0; 3],
            radius: 10.0,
        }
    }

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
    fn prf_13_resumed_marker_work_keeps_actor_ownership() {
        if profile_child("prf_13_resumed_marker_work_keeps_actor_ownership") {
            return;
        }
        use soe_trace::{Operation, Profile};
        Profile::disable();
        let mut world = fixture();
        let actor = id(0x810);
        let mut planner = Planner::default();
        planner.begin_frame(&mut world, &[actor]);
        assert!(planner.request(&mut world, actor, goal()).is_pending());
        Profile::clear();
        Profile::enable();
        planner.begin_frame(&mut world, &[actor]);
        Profile::disable();
        assert_eq!(planner.last_work, REQUEST_BUDGET);
        let rows = Profile::summary();
        let (_, stats) = rows
            .iter()
            .find(|(e, _)| {
                e.kind == "actor"
                    && e.id == actor.to_string()
                    && e.phase == "planning"
                    && e.plugin == "Prf11MarkerSlices.esm"
            })
            .unwrap();
        assert_eq!(stats.calls, 1);
        assert!(stats.total_ns > 0);
        assert!(rows.iter().any(|(e, s)| e.kind == "record"
            && e.plugin == "Prf11MarkerSlices.esm"
            && s.work.get(Operation::RecordLookups) > 0));
    }

    #[test]
    fn prf_11_marker_setup_and_sorted_index_merge_have_bounded_work() {
        use soe_esm::GroupKind;

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
        plugin.push_top(Record::new(Tag(*b"IDLM"), 0x801));
        for object in 0x1000..0x2000 {
            let tag = if object % 7 == 0 {
                Tag(*b"PHZD")
            } else {
                Tag(*b"REFR")
            };
            let mut record = Record::new(tag, object).sub(Tag(*b"NAME"), 0x801u32.to_le_bytes());
            record.placement.cell = Some(0x800);
            match object % 3 {
                0 => record.placement.group = Some(GroupKind::CellPersistent(0x800)),
                1 => record.placement.group = Some(GroupKind::CellTemporary(0x800)),
                _ => record.flags |= 0x20,
            }
            plugin.push_top(record);
        }
        let mut order = LoadOrder::new();
        order.add("Prf11MarkerMerge.esm", plugin).unwrap();
        let order = std::sync::Arc::new(order);
        order.build_cell_index();
        let mut oracle = World::from_shared_order(order.clone());
        let expected = super::super::sandbox_markers(&mut oracle, id(0x800), [0.0; 3], 10.0);
        let mut world = World::from_shared_order(order);
        let mut planner = Planner::default();
        let actor = id(0x810);
        planner.begin_frame(&mut world, &[actor]);
        assert!(planner.request(&mut world, actor, goal()).is_pending());
        assert_eq!(planner.last_work, REQUEST_BUDGET);
        assert_eq!(planner.requests[&actor].cursor, REQUEST_BUDGET);
        assert_eq!(planner.requests[&actor].total, 4096);
        assert!(
            world.reference_ref(id(0x1fff)).is_none(),
            "setup scanned the whole cell"
        );
        let mut result = Poll::Pending;
        for _ in 0..128 {
            planner.begin_frame(&mut world, &[actor]);
            result = planner.request(&mut world, actor, goal());
            assert!(planner.last_work <= REQUEST_BUDGET);
            if result.is_ready() {
                break;
            }
        }
        assert_eq!(result, Poll::Ready(expected));
    }

    #[test]
    fn prf_11_marker_slices_bound_work_keep_order_and_avoid_starvation() {
        let mut world = fixture();
        let expected = super::super::sandbox_markers(&mut world, id(0x800), [0.0; 3], 10.0);
        let mut planner = Planner::default();
        let actors: Vec<_> = (0x810..0x81c).map(id).collect();
        planner.begin_frame(&mut world, &actors);
        for actor in &actors {
            assert!(planner.request(&mut world, *actor, goal()).is_pending());
        }
        assert_eq!(planner.last_work, FRAME_BUDGET);
        let mut completed = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let before: BTreeMap<_, _> = planner
                .requests
                .iter()
                .map(|(actor, request)| (*actor, request.cursor))
                .collect();
            planner.begin_frame(&mut world, &actors);
            for (actor, request) in &planner.requests {
                assert!(
                    request.cursor - before.get(actor).copied().unwrap_or_default()
                        <= REQUEST_BUDGET
                );
            }
            for actor in &actors {
                if completed.contains(actor) {
                    continue;
                }
                if let Poll::Ready(matches) = planner.request(&mut world, *actor, goal()) {
                    assert_eq!(matches, expected);
                    completed.insert(*actor);
                }
            }
            assert!(planner.last_work <= FRAME_BUDGET);
            if completed.len() == actors.len() {
                break;
            }
        }
        assert_eq!(completed.len(), actors.len());
    }

    #[test]
    fn prf_11_pending_marker_query_retains_actor_step_until_ready() {
        use super::super::{step, AiState, Procedure};
        let mut world = fixture();
        world.ai.nav = Some(soe_navmesh::NavGraph::new());
        let actor = id(0x810);
        let before = AiState {
            procedure: Procedure::Sandbox,
            center: Some([0.0; 3]),
            center_cell: Some(id(0x800)),
            radius: 10.0,
            ..AiState::default()
        };
        world.actor(actor).unwrap().ai = before.clone();
        let mut planner = std::mem::take(&mut world.ai.markers);
        planner.begin_frame(&mut world, &[actor]);
        world.ai.markers = planner;
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        step(&mut world, actor, 0.1, &mut alloc);
        assert!(world.ai.markers.waiting(actor));
        assert_eq!(world.actor(actor).unwrap().ai, before);
        for _ in 0..10 {
            let mut planner = std::mem::take(&mut world.ai.markers);
            planner.begin_frame(&mut world, &[actor]);
            world.ai.markers = planner;
            assert_eq!(world.actor(actor).unwrap().ai, before);
            if !world.ai.markers.waiting(actor) {
                break;
            }
        }
        assert!(!world.ai.markers.waiting(actor));
        step(&mut world, actor, 0.1, &mut alloc);
        assert_eq!(world.actor(actor).unwrap().ai.destination, Some([0.0; 3]));
    }

    #[test]
    fn prf_11_marker_goal_change_cancels_old_candidates() {
        let mut world = fixture();
        let mut planner = Planner::default();
        let actor = id(0x810);
        planner.begin_frame(&mut world, &[actor]);
        assert!(planner.request(&mut world, actor, goal()).is_pending());
        let changed = Goal {
            cell: id(0x999),
            ..goal()
        };
        assert_eq!(
            planner.request(&mut world, actor, changed),
            Poll::Ready(Vec::new())
        );
        assert!(!planner.waiting(actor));
    }
}

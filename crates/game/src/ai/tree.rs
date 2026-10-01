// SPDX-License-Identifier: GPL-3.0-or-later

//! Sequential, random and stacked PACK execution from xEdit's PRCB counts and repeat flag.
//! The author test in Nexus thread 11591393 confirms that nested repeat
//! restarts that branch alone. Progress belongs to the actor, definitions to PACK.
//! Random single-child execution follows the CK examples in Nexus article 2467
//! and thread 804089. Choices use the existing saved world RNG.

use super::{conditions_hold, package, template_chain, AiState, Procedure, World};
use crate::combat::Hand;
use crate::package::{branch_ends, Branch, DataValue, LocationData, Package, Target};
use crate::world::Alloc;
use soe_esm::GlobalFormId;

/// A cursor into the preorder branches of a winning template.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeState {
    pub owner: GlobalFormId,
    pub branch: u32,
    pub active: bool,
    pub finished: bool,
    pub stacked: Vec<StackedState>,
    pub stacked_completed: Vec<u32>,
}

/// A lower-priority child paused while an earlier Stacked child runs.
#[derive(Debug, Clone, PartialEq)]
pub struct StackedState {
    pub(super) parent: u32,
    pub(super) branch: u32,
    pub(super) active: bool,
    pub(super) execution: ExecutionState,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ExecutionState {
    pub(super) procedure: Procedure,
    pub(super) destination: Option<[f32; 3]>,
    pub(super) path: Vec<[f32; 3]>,
    pub(super) path_cells: Vec<Option<GlobalFormId>>,
    pub(super) arrived: bool,
    pub(super) sleeping: bool,
    pub(super) bed: Option<GlobalFormId>,
    pub(super) center: Option<[f32; 3]>,
    pub(super) center_cell: Option<GlobalFormId>,
    pub(super) radius: f32,
    pub(super) dwell_until: f64,
    pub(super) wait_until: Option<f64>,
    pub(super) patrol: Option<super::PatrolState>,
    pub(super) speed: f32,
    pub(super) start_position: [f32; 3],
}

struct Tree {
    owner: GlobalFormId,
    branches: Vec<Branch>,
    ends: Vec<usize>,
    parents: Vec<Option<usize>>,
    chain: Vec<(GlobalFormId, Package)>,
}

impl Tree {
    fn load(world: &World, id: GlobalFormId) -> Option<Self> {
        let chain = template_chain(world, id);
        let (owner, template) = chain.iter().find(|(_, p)| !p.tree.is_empty())?;
        let ends = branch_ends(&template.tree).ok()?;
        let mut parents = vec![None; ends.len()];
        let mut stack = Vec::new();
        for (index, parent_slot) in parents.iter_mut().enumerate() {
            while stack.last().is_some_and(|&parent| ends[parent] <= index) {
                stack.pop();
            }
            *parent_slot = stack.last().copied();
            stack.push(index);
        }
        Some(Self {
            owner: *owner,
            branches: template.tree.clone(),
            ends,
            parents,
            chain,
        })
    }

    fn location(&self, branch: &Branch) -> Option<(GlobalFormId, LocationData)> {
        branch.inputs.iter().find_map(|&index| {
            self.chain
                .iter()
                .find_map(|(owner, p)| match p.input(index as i8).map(|i| i.value) {
                    Some(DataValue::Location(l)) => Some((*owner, l)),
                    _ => None,
                })
        })
    }

    // CONTEXT: All 87 vanilla Wait branches bind seconds before the movement flag.
    fn wait_parameters(&self, branch: &Branch) -> Option<(f32, bool)> {
        let [seconds, stop] = branch.inputs.as_slice() else {
            return None;
        };
        let input = |index: u8| {
            self.chain
                .iter()
                .find_map(|(_, p)| p.input(index as i8).map(|input| input.value))
        };
        match (input(*seconds), input(*stop)) {
            (Some(DataValue::Float(seconds)), Some(DataValue::Bool(stop)))
                if seconds.is_finite() && seconds >= 0.0 =>
            {
                Some((seconds, stop))
            }
            _ => None,
        }
    }

    fn hover_parameters(&self, branch: &Branch) -> Option<f32> {
        let height = branch.inputs.iter().find_map(|&index| {
            self.chain.iter().find_map(|(_, package)| {
                match package.input(index as i8).map(|input| input.value) {
                    Some(DataValue::Float(height)) if height.is_finite() => Some(height),
                    _ => None,
                }
            })
        })?;
        self.location(branch).map(|_| height)
    }

    fn orbit_parameters(&self, branch: &Branch) -> Option<f32> {
        let values = branch
            .inputs
            .iter()
            .filter_map(|&index| {
                self.chain.iter().find_map(|(_, package)| {
                    match package.input(index as i8).map(|input| input.value) {
                        Some(DataValue::Float(value)) => Some(value),
                        _ => None,
                    }
                })
            })
            .collect::<Vec<_>>();
        let [inner, outer, height] = values.as_slice() else {
            return None;
        };
        (inner.is_finite()
            && outer.is_finite()
            && height.is_finite()
            && *inner >= 0.0
            && *outer >= *inner)
            .then_some(*height)
            .filter(|_| self.location(branch).is_some())
    }

    fn simultaneous_wait(
        &self,
        parent: usize,
        actor: GlobalFormId,
        world: &mut World,
        quest: Option<GlobalFormId>,
    ) -> Option<(f32, bool, Option<u32>)> {
        let mut child = parent + 1;
        let mut duration = 0.0_f32;
        let mut success_duration = None;
        let mut success_branch = None;
        let mut stop_movement = false;
        let mut indefinite = false;
        let mut found = false;
        while child < self.ends[parent] {
            let branch = self.branches.get(child)?;
            if branch.child_count != 0 || !branch.kind.eq_ignore_ascii_case("procedure") {
                return None;
            }
            if conditions_hold(world, actor, self.owner, quest, &branch.conditions) {
                let (seconds, stop) = self.wait_parameters(branch)?;
                duration = duration.max(seconds);
                indefinite |= seconds == 0.0;
                if branch.success_completes_package && seconds > 0.0 {
                    let replace = success_duration.is_none_or(|current| seconds < current);
                    if replace {
                        success_duration = Some(seconds);
                        success_branch = Some(child as u32);
                    }
                }
                stop_movement |= stop;
                found = true;
            }
            child = self.ends[child];
        }
        if let Some(seconds) = success_duration {
            return Some((seconds, stop_movement, success_branch));
        }
        found.then_some((if indefinite { 0.0 } else { duration }, stop_movement, None))
    }

    fn patrol_parameters(
        &self,
        branch: &Branch,
    ) -> Result<super::patrol::Parameters, &'static str> {
        let inputs = (0..6)
            .map(|position| {
                let index = branch.inputs.get(position).copied().unwrap_or(u8::MAX);
                if index == u8::MAX {
                    return match position {
                        2..=5 => Some((self.owner, DataValue::Bool(false))),
                        _ => None,
                    };
                }
                self.chain.iter().find_map(|(owner, package)| {
                    package
                        .input(index as i8)
                        .map(|input| (*owner, input.value))
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("Patrol omitted required input, AI-90")?;
        super::patrol::Parameters::from_inputs(&inputs)
    }

    fn after(&self, mut index: usize, state: &mut TreeState) {
        loop {
            let next = self.ends[index];
            let Some(parent) = self.parents[index] else {
                state.branch = next as u32;
                state.finished = next == self.branches.len();
                return;
            };
            let branch = &self.branches[parent];
            if !branch.kind.eq_ignore_ascii_case("random") && next < self.ends[parent] {
                state.branch = next as u32;
                return;
            }
            if branch.repeat_when_complete {
                state.branch = parent as u32;
                return;
            }
            index = parent;
        }
    }

    fn stacked_parent(&self, index: usize) -> Option<(usize, usize)> {
        let mut child = index;
        while let Some(parent) = self.parents.get(child).copied().flatten() {
            if self.branches[parent].kind.eq_ignore_ascii_case("stacked") {
                return Some((parent, child));
            }
            child = parent;
        }
        None
    }

    fn child_under(&self, index: usize, parent: usize) -> Option<usize> {
        let mut child = index;
        while let Some(ancestor) = self.parents.get(child).copied().flatten() {
            if ancestor == parent {
                return Some(child);
            }
            child = ancestor;
        }
        None
    }
}

impl ExecutionState {
    pub(super) fn capture(ai: &AiState) -> Self {
        Self {
            procedure: ai.procedure.clone(),
            destination: ai.destination,
            path: ai.path.clone(),
            path_cells: ai.path_cells.clone(),
            arrived: ai.arrived,
            sleeping: ai.sleeping,
            bed: ai.bed,
            center: ai.center,
            center_cell: ai.center_cell,
            radius: ai.radius,
            dwell_until: ai.dwell_until,
            wait_until: ai.wait_until,
            patrol: ai.patrol.clone(),
            speed: ai.speed,
            start_position: ai.start_position,
        }
    }

    fn restore(self, ai: &mut AiState) {
        ai.procedure = self.procedure;
        ai.destination = self.destination;
        ai.path = self.path;
        ai.path_cells = self.path_cells;
        ai.arrived = self.arrived;
        ai.sleeping = self.sleeping;
        ai.bed = self.bed;
        ai.center = self.center;
        ai.center_cell = self.center_cell;
        ai.radius = self.radius;
        ai.dwell_until = self.dwell_until;
        ai.wait_until = self.wait_until;
        ai.patrol = self.patrol;
        ai.speed = self.speed;
        ai.start_position = self.start_position;
    }
}

fn procedure(branch: &Branch) -> Procedure {
    match branch
        .procedure
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("travel") => Procedure::Travel,
        Some("sandbox") => Procedure::Sandbox,
        Some("sleep") => Procedure::Sleep,
        Some("flee") => Procedure::Flee,
        Some("patrol") => Procedure::Patrol,
        Some("guard") => Procedure::Guard,
        Some("sit") => Procedure::Sit,
        Some("usemagic") => Procedure::UseMagic,
        Some("useweapon") => Procedure::UseWeapon,
        Some("wait") => Procedure::Wait {
            stop_movement: false,
        },
        Some("hover") => Procedure::Hover,
        Some("follow") => Procedure::Follow,
        Some("escort") => Procedure::Escort,
        Some("orbit") => Procedure::Orbit { height: 0.0 },
        Some("forcegreet") => Procedure::ForceGreet,
        Some("dialogueactivate") => Procedure::DialogueActivate,
        Some("useidlemarker") => Procedure::UseIdleMarker { marker: None },
        Some("activate") => Procedure::Activate,
        Some("find") => Procedure::Find,
        Some("holdposition") => Procedure::HoldPosition,
        Some("shout") => Procedure::Shout,
        Some(_) => Procedure::Unsupported(branch.procedure.clone().unwrap_or_default()),
        None => Procedure::Unsupported(branch.kind.clone()),
    }
}

pub(super) fn follow_parameters(
    world: &World,
    ai: &AiState,
) -> Option<(GlobalFormId, crate::package::Target, f32, f32)> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    let target_input = branch.inputs.iter().find_map(|&index| {
        tree.chain.iter().find_map(|(owner, package)| {
            match package.input(index as i8).map(|input| input.value) {
                Some(DataValue::Target(target)) => Some((*owner, target.target)),
                _ => None,
            }
        })
    })?;
    let radii: Vec<f32> = branch
        .inputs
        .iter()
        .filter_map(|&index| {
            tree.chain.iter().find_map(|(_, package)| {
                match package.input(index as i8).map(|input| input.value) {
                    Some(DataValue::Float(value)) => Some(value),
                    _ => None,
                }
            })
        })
        .collect();
    let catch_up_radius = radii.first().copied().unwrap_or(20.0);
    let follow_radius = radii.get(1).copied().unwrap_or(5.0);
    if !catch_up_radius.is_finite()
        || !follow_radius.is_finite()
        || catch_up_radius < follow_radius
        || follow_radius < 0.0
    {
        return None;
    }
    Some((
        target_input.0,
        target_input.1,
        catch_up_radius,
        follow_radius,
    ))
}

pub(super) fn escort_target(
    world: &World,
    ai: &AiState,
) -> Option<(GlobalFormId, crate::package::Target, i32)> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    branch.inputs.iter().find_map(|&index| {
        tree.chain.iter().find_map(|(owner, package)| {
            match package.input(index as i8).map(|input| input.value) {
                Some(DataValue::Target(target)) => Some((*owner, target.target, target.count)),
                _ => None,
            }
        })
    })
}

pub(super) fn orbit_parameters(
    world: &World,
    ai: &AiState,
) -> Option<(GlobalFormId, LocationData, f32, f32, f32)> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    let (owner, location) = tree.location(branch)?;
    let values = branch
        .inputs
        .iter()
        .filter_map(|&index| {
            tree.chain.iter().find_map(|(_, package)| {
                match package.input(index as i8).map(|input| input.value) {
                    Some(DataValue::Float(value)) => Some(value),
                    _ => None,
                }
            })
        })
        .collect::<Vec<_>>();
    let [inner, outer, height] = values.as_slice() else {
        return None;
    };
    if !inner.is_finite()
        || !outer.is_finite()
        || !height.is_finite()
        || *inner < 0.0
        || *outer < *inner
    {
        return None;
    }
    Some((owner, location, *inner, *outer, *height))
}

pub(super) fn force_greet_without_inputs(world: &World, ai: &AiState) -> bool {
    let Some(tree) = ai.package.and_then(|id| Tree::load(world, id)) else {
        return false;
    };
    ai.tree
        .as_ref()
        .and_then(|cursor| tree.branches.get(cursor.branch as usize))
        .is_some_and(|branch| branch.inputs.is_empty())
}

pub(super) fn force_greet_parameters(
    world: &World,
    ai: &AiState,
) -> Option<((GlobalFormId, crate::package::Target), GlobalFormId)> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    let mut target = None;
    let mut topic = None;
    for &index in &branch.inputs {
        for (owner, package) in &tree.chain {
            match package.input(index as i8).map(|input| input.value) {
                Some(DataValue::Target(target_data)) if target.is_none() => {
                    target = Some((*owner, target_data.target));
                }
                Some(DataValue::Topic(crate::package::Topic::Dialogue(raw))) if topic.is_none() => {
                    topic = world
                        .resolve_from(*owner, raw)
                        .filter(|id| world.tag_of(*id) == Some(soe_esm::Tag::new(b"DIAL")));
                }
                _ => {}
            }
        }
    }
    Some((target?, topic?))
}

pub(super) fn activate_target(
    world: &World,
    ai: &AiState,
) -> Option<(GlobalFormId, crate::package::Target)> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    branch.inputs.iter().find_map(|&index| {
        tree.chain.iter().find_map(|(owner, package)| {
            match package.input(index as i8).map(|input| input.value) {
                Some(DataValue::Target(target)) => Some((*owner, target.target)),
                _ => None,
            }
        })
    })
}

type FindTarget = (
    GlobalFormId,
    crate::package::Target,
    Option<(GlobalFormId, LocationData)>,
);

pub(super) fn find_target(world: &World, ai: &AiState) -> Option<FindTarget> {
    let package_id = ai.package?;
    let tree = Tree::load(world, package_id)?;
    let branch = tree.branches.get(ai.tree.as_ref()?.branch as usize)?;
    let target = branch.inputs.iter().find_map(|&index| {
        tree.chain.iter().find_map(|(owner, package)| {
            match package.input(index as i8).map(|input| input.value) {
                Some(DataValue::Target(target)) => Some((*owner, target.target)),
                _ => None,
            }
        })
    })?;
    Some((target.0, target.1, tree.location(branch)))
}

/// Static first procedure for tooling. Random choices have no static first leaf.
pub(super) fn first(world: &World, id: GlobalFormId) -> (Procedure, Option<LocationData>, bool) {
    let Some(tree) = Tree::load(world, id) else {
        return (Procedure::None, None, false);
    };
    for branch in &tree.branches {
        if branch.kind.eq_ignore_ascii_case("random") && branch.procedure.is_none() {
            return (Procedure::None, None, false);
        }
        if branch.kind.eq_ignore_ascii_case("sequence") && branch.procedure.is_none() {
            continue;
        }
        let chosen = match procedure(branch) {
            Procedure::Patrol => tree.patrol_parameters(branch).map_or_else(
                |reason| Procedure::Unsupported(reason.into()),
                |_| Procedure::Patrol,
            ),
            Procedure::Wait { .. } => tree.wait_parameters(branch).map_or_else(
                || Procedure::Unsupported("Wait inputs".into()),
                |(_, stop_movement)| Procedure::Wait { stop_movement },
            ),
            Procedure::Hover => tree.hover_parameters(branch).map_or_else(
                || Procedure::Unsupported("Hover inputs, AI-31".into()),
                |_| Procedure::Hover,
            ),
            Procedure::Follow => Procedure::Follow,
            other => other,
        };
        return (
            chosen,
            tree.location(branch).map(|(_, l)| l),
            branch.success_completes_package,
        );
    }
    (Procedure::None, None, false)
}

pub(super) fn start(world: &mut World, actor: GlobalFormId, ai: &mut AiState, alloc: Alloc<'_>) {
    ai.tree = ai
        .package
        .and_then(|id| Tree::load(world, id))
        .map(|tree| TreeState {
            owner: tree.owner,
            branch: 0,
            active: false,
            finished: false,
            stacked: Vec::new(),
            stacked_completed: Vec::new(),
        });
    enter(world, actor, ai, alloc);
}

pub(super) fn enter(world: &mut World, actor: GlobalFormId, ai: &mut AiState, alloc: Alloc<'_>) {
    let Some(mut state) = ai.tree.take() else {
        return;
    };
    if state.finished {
        ai.tree = Some(state);
        return;
    }
    let Some(tree) = ai.package.and_then(|id| Tree::load(world, id)) else {
        ai.procedure = Procedure::Unsupported("missing procedure tree".into());
        ai.tree = Some(state);
        return;
    };
    if state.owner != tree.owner || state.branch as usize > tree.branches.len() {
        ai.procedure = Procedure::Unsupported("changed procedure tree".into());
        ai.tree = Some(state);
        return;
    }
    if state.active {
        if let Some((parent, active_child)) = tree.stacked_parent(state.branch as usize) {
            let mut child = parent + 1;
            while child < active_child {
                let next = tree.ends[child];
                if !state.stacked_completed.contains(&(child as u32))
                    && conditions_hold(
                        world,
                        actor,
                        tree.owner,
                        quest_for(world, ai),
                        &tree.branches[child].conditions,
                    )
                {
                    state.stacked.push(StackedState {
                        parent: parent as u32,
                        branch: state.branch,
                        active: true,
                        execution: ExecutionState::capture(ai),
                    });
                    state.branch = child as u32;
                    state.active = false;
                    break;
                }
                child = next;
            }
        }
    }
    if state.active {
        ai.tree = Some(state);
        return;
    }
    // CONTEXT: The driver holds a working copy while branch conditions query
    // the actor's current package and sleeping state through the shared world.
    if let Some(actor_state) = world.actor(actor) {
        actor_state.ai = ai.clone();
        actor_state.ai.tree = Some(state.clone());
    }
    let quest = quest_for(world, ai);
    let mut checked = None;
    let mut use_magic_succeeded = false;
    // CONTEXT: An empty repeating branch must yield rather than spin in one tick.
    for _ in 0..=tree.branches.len() {
        if state.finished {
            break;
        }
        let index = state.branch as usize;
        let Some(branch) = tree.branches.get(index) else {
            state.finished = true;
            break;
        };
        if checked.take() != Some(index)
            && !conditions_hold(world, actor, tree.owner, quest, &branch.conditions)
        {
            tree.after(index, &mut state);
            continue;
        }
        if branch.kind.eq_ignore_ascii_case("random") && branch.procedure.is_none() {
            let mut eligible = Vec::new();
            let mut child = index + 1;
            while child < tree.ends[index] {
                if conditions_hold(
                    world,
                    actor,
                    tree.owner,
                    quest,
                    &tree.branches[child].conditions,
                ) {
                    eligible.push(child);
                }
                child = tree.ends[child];
            }
            if eligible.is_empty() {
                if branch.repeat_when_complete {
                    break;
                }
                tree.after(index, &mut state);
            } else {
                let pick = (world.random_u64() % eligible.len() as u64) as usize;
                let child = eligible[pick];
                state.branch = child as u32;
                // CONTEXT: RandomPercent conditions must not roll again on entry.
                checked = Some(child);
            }
            continue;
        }
        if branch.kind.eq_ignore_ascii_case("stacked") && branch.procedure.is_none() {
            let mut child = index + 1;
            let mut selected = None;
            while child < tree.ends[index] {
                if !state.stacked_completed.contains(&(child as u32))
                    && conditions_hold(
                        world,
                        actor,
                        tree.owner,
                        quest,
                        &tree.branches[child].conditions,
                    )
                {
                    selected = Some(child);
                    break;
                }
                child = tree.ends[child];
            }
            if let Some(child) = selected {
                state.branch = child as u32;
                checked = Some(child);
            } else if branch.repeat_when_complete {
                break;
            } else {
                tree.after(index, &mut state);
            }
            continue;
        }
        if branch.kind.eq_ignore_ascii_case("simultaneous") && branch.procedure.is_none() {
            match tree.simultaneous_wait(index, actor, world, quest) {
                Some((seconds, stop_movement, completion_branch)) => {
                    if let Some(branch) = completion_branch {
                        state.branch = branch;
                    }
                    state.active = true;
                    let deadline =
                        (seconds > 0.0).then_some(world.clock.real_seconds + f64::from(seconds));
                    super::begin_procedure(
                        world,
                        actor,
                        ai,
                        Procedure::Wait { stop_movement },
                        None,
                        None,
                    );
                    ai.wait_until = deadline;
                }
                None => {
                    ai.procedure = Procedure::Unsupported(
                        "Simultaneous supports only Wait children until AI-87 completion rules are available".into(),
                    );
                    state.active = true;
                }
            }
            break;
        }
        if branch.kind.eq_ignore_ascii_case("sequence") && branch.procedure.is_none() {
            if branch.child_count == 0 {
                if branch.repeat_when_complete {
                    break;
                }
                tree.after(index, &mut state);
            } else {
                state.branch += 1;
            }
            continue;
        }
        state.active = true;
        let mut chosen = procedure(branch);
        let mut location = tree.location(branch);
        let mut hover_height = None;
        let mut wait_until = None;
        if matches!(chosen, Procedure::Wait { .. }) {
            match tree.wait_parameters(branch) {
                Some((seconds, stop_movement)) if world.clock.real_seconds.is_finite() => {
                    chosen = Procedure::Wait { stop_movement };
                    if seconds > 0.0 {
                        wait_until = Some(world.clock.real_seconds + f64::from(seconds));
                    }
                }
                _ => chosen = Procedure::Unsupported("Wait inputs or clock".into()),
            }
        }
        if chosen == Procedure::Hover {
            match tree.hover_parameters(branch) {
                Some(height) => hover_height = Some(height),
                None => chosen = Procedure::Unsupported("Hover inputs, AI-31".into()),
            }
        }
        if matches!(chosen, Procedure::Orbit { .. }) {
            match tree.orbit_parameters(branch) {
                Some(height) => chosen = Procedure::Orbit { height },
                None => chosen = Procedure::Unsupported("Orbit inputs, AI-25".into()),
            }
        }
        let is_patrol = chosen == Procedure::Patrol;
        ai.tree = Some(state.clone());
        super::begin_procedure(world, actor, ai, chosen, location.take(), hover_height);
        if is_patrol {
            super::patrol::begin(world, actor, ai, tree.patrol_parameters(branch));
        }
        ai.wait_until = wait_until;
        if ai.procedure == Procedure::UseMagic {
            match use_magic(world, actor, ai, &tree, branch, alloc) {
                Ok(()) => {
                    use_magic_succeeded = true;
                    ai.procedure = Procedure::None;
                }
                Err(reason) => ai.procedure = Procedure::Unsupported(reason.into()),
            }
        }
        break;
    }
    ai.tree = Some(state);
    if use_magic_succeeded {
        succeeded(world, ai);
    }
    if ai.tree.as_ref().is_some_and(|s| s.finished) {
        complete(world, ai);
    }
}

fn use_magic(
    world: &mut World,
    actor: GlobalFormId,
    ai: &AiState,
    tree: &Tree,
    branch: &Branch,
    alloc: Alloc<'_>,
) -> Result<(), &'static str> {
    let (owner, target) = branch
        .inputs
        .iter()
        .find_map(|input| {
            tree.chain.iter().find_map(|(owner, package)| {
                let value = package.input(*input as i8)?.value;
                match value {
                    DataValue::Target(target) => Some((*owner, target.target)),
                    _ => None,
                }
            })
        })
        .ok_or("UseMagic target input is missing")?;
    let target = match target {
        Target::Reference(raw) => world.resolve_from(owner, raw),
        Target::Itself => Some(actor),
        Target::LinkedReference(raw_keyword) => {
            let keyword = (raw_keyword != 0)
                .then(|| world.resolve_from(owner, raw_keyword))
                .flatten();
            let mut links = world
                .reference(actor)
                .into_iter()
                .flat_map(|state| state.linked.iter())
                .filter(|(kind, _)| kind.is_none_or(|kind| Some(kind) == keyword));
            let first = links.next().map(|(_, target)| *target);
            if links.next().is_some() {
                None
            } else {
                first
            }
        }
        Target::Alias(id) => ai
            .alias
            .and_then(|(quest, _)| world.quest(quest)?.aliases.get(&id).copied().flatten()),
        _ => None,
    }
    .ok_or("UseMagic target did not resolve")?;
    if !world
        .actor(target)
        .is_some_and(|state| !state.dead && !state.ghost)
    {
        return Err("UseMagic target is not an eligible actor");
    }
    let spell = [Hand::Right, Hand::Left]
        .into_iter()
        .find_map(|hand| {
            let spell = world.hand_spell(actor, hand)?;
            world
                .spell(spell)
                .ok()
                .filter(|definition| definition.data.spell_type == 0)?;
            Some(spell)
        })
        .ok_or("UseMagic actor has no assigned castable spell")?;
    world
        .cast_charged_spell(spell, actor, actor, Some(target), alloc)
        .map_err(|_| "UseMagic assigned spell could not be cast")?;
    Ok(())
}

pub(super) fn patrol_parameters(
    world: &World,
    ai: &AiState,
) -> Result<super::patrol::Parameters, &'static str> {
    let tree = ai
        .package
        .and_then(|id| Tree::load(world, id))
        .ok_or("missing Patrol package")?;
    let state = ai.tree.as_ref().ok_or("missing Patrol cursor")?;
    let branch = tree
        .branches
        .get(state.branch as usize)
        .ok_or("invalid Patrol cursor")?;
    tree.patrol_parameters(branch)
}

fn quest_for(world: &World, ai: &AiState) -> Option<GlobalFormId> {
    ai.alias.map(|(quest, _)| quest).or_else(|| {
        let id = ai.package?;
        package(world, id)?
            .owner_quest
            .and_then(|raw| world.resolve_from(id, raw))
    })
}

pub(super) fn succeeded(world: &World, ai: &mut AiState) {
    let Some(mut state) = ai.tree.take() else {
        return;
    };
    if !state.active || state.finished {
        ai.tree = Some(state);
        return;
    }
    let Some(tree) = ai.package.and_then(|id| Tree::load(world, id)) else {
        ai.tree = Some(state);
        return;
    };
    let index = state.branch as usize;
    if let Some(branch) = tree.branches.get(index) {
        let mut stacked_children = Vec::new();
        let mut child = index;
        while let Some(parent) = tree.parents.get(child).copied().flatten() {
            if tree.branches[parent].kind.eq_ignore_ascii_case("stacked") {
                stacked_children.push(child);
            }
            child = parent;
        }
        state.active = false;
        if branch.success_completes_package {
            state.finished = true;
        } else {
            tree.after(index, &mut state);
            for child in stacked_children {
                if state.branch as usize >= tree.ends[child] {
                    state.stacked_completed.push(child as u32);
                }
            }
            if let Some(parent) = tree.branches.get(state.branch as usize) {
                if parent.kind.eq_ignore_ascii_case("stacked") {
                    let parent_index = state.branch as usize;
                    state.stacked_completed.retain(|child| {
                        *child as usize <= parent_index
                            || *child as usize >= tree.ends[parent_index]
                    });
                }
            }
            while let Some(frame) = state.stacked.last() {
                let parent = frame.parent as usize;
                let Some(child) = tree.child_under(index, parent) else {
                    break;
                };
                if (state.branch as usize) < tree.ends[child] {
                    break;
                }
                let frame = state.stacked.pop().unwrap_or_else(|| unreachable!());
                state.branch = frame.branch;
                state.active = frame.active;
                state.finished = false;
                frame.execution.restore(ai);
            }
        }
    }
    ai.tree = Some(state);
    if ai.tree.as_ref().is_some_and(|s| s.finished) {
        complete(world, ai);
    }
}

fn complete(world: &World, ai: &mut AiState) {
    if let Some(id) = ai.package {
        ai.completed
            .insert(id, world.clock.game_days.floor() as u32);
    }
}

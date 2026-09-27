// SPDX-License-Identifier: GPL-3.0-or-later

//! Sequential and random PACK execution from xEdit's PRCB counts and repeat flag.
//! The author test in Nexus thread 11591393 confirms that nested repeat
//! restarts that branch alone. Progress belongs to the actor, definitions to PACK.
//! Random single-child execution follows the CK examples in Nexus article 2467
//! and thread 804089. Choices use the existing saved world RNG.

use super::{conditions_hold, package, template_chain, AiState, Procedure, World};
use crate::package::{branch_ends, Branch, DataValue, LocationData, Package};
use soe_esm::GlobalFormId;

/// A cursor into the preorder branches of a winning template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeState {
    pub owner: GlobalFormId,
    pub branch: u32,
    pub active: bool,
    pub finished: bool,
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

    fn patrol_parameters(
        &self,
        branch: &Branch,
    ) -> Result<super::patrol::Parameters, &'static str> {
        let inputs = branch
            .inputs
            .iter()
            .map(|&index| {
                self.chain.iter().find_map(|(owner, package)| {
                    package
                        .input(index as i8)
                        .map(|input| (*owner, input.value))
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("Patrol omitted input, AI-90")?;
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
        Some("patrol") => Procedure::Patrol,
        Some("wait") => Procedure::Wait {
            stop_movement: false,
        },
        Some("hover") => Procedure::Hover,
        Some(_) => Procedure::Unsupported(branch.procedure.clone().unwrap_or_default()),
        None => Procedure::Unsupported(branch.kind.clone()),
    }
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

pub(super) fn start(world: &mut World, actor: GlobalFormId, ai: &mut AiState) {
    ai.tree = ai
        .package
        .and_then(|id| Tree::load(world, id))
        .map(|tree| TreeState {
            owner: tree.owner,
            branch: 0,
            active: false,
            finished: false,
        });
    enter(world, actor, ai);
}

pub(super) fn enter(world: &mut World, actor: GlobalFormId, ai: &mut AiState) {
    let Some(mut state) = ai.tree.take() else {
        return;
    };
    if state.active || state.finished {
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
    // CONTEXT: The driver holds a working copy while branch conditions query
    // the actor's current package and sleeping state through the shared world.
    if let Some(actor_state) = world.actor(actor) {
        actor_state.ai = ai.clone();
        actor_state.ai.tree = Some(state.clone());
    }
    let quest = ai.alias.map(|(q, _)| q).or_else(|| {
        let id = ai.package?;
        package(world, id)?
            .owner_quest
            .and_then(|raw| world.resolve_from(id, raw))
    });
    let mut checked = None;
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
        let is_patrol = chosen == Procedure::Patrol;
        super::begin_procedure(world, actor, ai, chosen, location.take(), hover_height);
        if is_patrol {
            super::patrol::begin(world, actor, ai, tree.patrol_parameters(branch));
        }
        ai.wait_until = wait_until;
        break;
    }
    ai.tree = Some(state);
    if ai.tree.as_ref().is_some_and(|s| s.finished) {
        complete(world, ai);
    }
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
        state.active = false;
        if branch.success_completes_package {
            state.finished = true;
        } else {
            tree.after(index, &mut state);
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

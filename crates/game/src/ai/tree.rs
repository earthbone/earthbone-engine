// SPDX-License-Identifier: GPL-3.0-or-later

//! Sequential and random PACK execution from xEdit's PRCB counts and repeat flag.
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

    fn simultaneous_wait(
        &self,
        parent: usize,
        actor: GlobalFormId,
        world: &mut World,
        quest: Option<GlobalFormId>,
    ) -> Option<(f32, bool)> {
        let mut child = parent + 1;
        let mut duration = 0.0_f32;
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
                stop_movement |= stop;
                found = true;
            }
            child = self.ends[child];
        }
        found.then_some((if indefinite { 0.0 } else { duration }, stop_movement))
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
        Some("flee") => Procedure::Flee,
        Some("patrol") => Procedure::Patrol,
        Some("guard") => Procedure::Guard,
        Some("sit") => Procedure::Sit,
        Some("usemagic") => Procedure::UseMagic,
        Some("useweapon") => Procedure::UseWeapon,
        Some("wait") => Procedure::Wait {
            stop_movement: false,
        },
        Some("shout") => Procedure::Shout,
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
        });
    enter(world, actor, ai, alloc);
}

pub(super) fn enter(world: &mut World, actor: GlobalFormId, ai: &mut AiState, alloc: Alloc<'_>) {
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
        if branch.kind.eq_ignore_ascii_case("simultaneous") && branch.procedure.is_none() {
            match tree.simultaneous_wait(index, actor, world, quest) {
                Some((seconds, stop_movement)) => {
                    state.active = true;
                    let deadline =
                        (seconds > 0.0).then_some(world.clock.real_seconds + f64::from(seconds));
                    super::begin_procedure(
                        world,
                        actor,
                        ai,
                        Procedure::Wait { stop_movement },
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
        let is_patrol = chosen == Procedure::Patrol;
        super::begin_procedure(world, actor, ai, chosen, tree.location(branch));
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

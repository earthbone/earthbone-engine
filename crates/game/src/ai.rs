// SPDX-License-Identifier: GPL-3.0-or-later

//! Package evaluation and the actor driver. An actor's package stack is its
//! quest alias packages (running quests first, by priority) followed by the
//! `PKID` list of its base record, and the first package whose schedule and
//! conditions hold wins, the way the Creation Kit wiki describes the stack
//! and `EvaluatePackage` (`engine-refs/ck-wiki/EvaluatePackage_-_Actor`).
//! PACK begin, end and change fragments follow xEdit's `wbVMADFragmentedPACK` order, with the actor argument described by the CK wiki's Papyrus Fragments section.
//! The chosen package's template names a procedure, and four of them move
//! the actor on the clock: `Travel` walks to a place, `Sandbox` wanders
//! between idle markers around a place, `Sleep` walks to a bed and holds,
//! and `Sit` reserves furniture before walking to it.
//! `Wait` uses the non-menu clock and holds until its duration expires.
//! `Hover` flies to its authored location and height, then holds there until
//! another package replaces it. `Follow` tracks a resolved target, stopping
//! within its authored follow radius and running beyond its catch-up radius.
//! `Orbit` circles a resolved package location at its authored height and
//! stays between its inner and outer radii on the navmesh.
//! `Activate` walks to its resolved target and activates it once through the
//! game's shared activation path.
//! `DialogueActivate` starts a conversation with its resolved actor target
//! once it is within the configured dialogue range.
//! `Find` selects the nearest enabled placed reference that matches its
//! authored object input in the search cell.
//! `UseMagic` resolves a package target and casts an assigned hand spell once
//! through the charged magic path.
//! Walking follows the vanilla navmesh graph (`soe_navmesh::vanilla`)
//! built once per world, and crossing into another navmesh moves the actor
//! into that navmesh's cell. Its rates come from the actor race's `WKMV` and
//! `RNMV` movement types, with matching `MTYP`/`SPED` overrides. The layouts
//! are from xEdit `wbRecord(RACE)` and Mutagen `Race.xml`.
//! `Hover` reads a package location and height from `wbRecord(PACK)` inputs;
//! the CK wiki defines its active flight state through `GetFlyingState` and
//! `IsFlying`.
//!
//! Conditions are the `CTDA` subset listed in [`function_value`], the
//! function indexes from xEdit's `wbConditionFunctions`. A function outside
//! the subset fails its condition and is counted in
//! [`AiWorld::unsupported_functions`] so a live run can say what is missing.

use std::collections::{BTreeMap, HashMap};

use soe_esm::records::navm::NavMesh;
use soe_esm::{GlobalFormId, Tag};
use soe_navmesh::vanilla::{key_of, Builder};
use soe_navmesh::NavGraph;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::{Value, VmError};

use crate::actor_values::actor_value_name;
use crate::clock::GameClock;
use crate::package::{
    condition, flags, speed, CompValue, Condition, Location, LocationData, Package, Schedule,
};
use crate::vmad::Vmad;
use crate::world::save::{read_form_id, read_opt_form_id, FormNamespace, FormReader};
use crate::world::{Alloc, Handle, Pending, World};

mod patrol;
mod tree;
pub use patrol::PatrolState;
pub use tree::TreeState;

/// Units per real second used when a race lacks a usable movement type.
///
/// These live in `soe_physics` because the character controller is the lower
/// crate and cannot depend on the runtime that drives it. They are re-exported
/// here so the package driver and the controller cannot drift apart, which
/// they had begun to do: both crates declared their own copies until
/// 2026-09-18.
pub use soe_physics::{FAST_WALK_SPEED, JOG_SPEED, RUN_SPEED, WALK_SPEED};

/// Closer than this to a waypoint counts as reaching it.
const ARRIVAL_DISTANCE: f32 = 4.0;
/// Real seconds between two evaluations of an actor that has a package.
const EVALUATION_INTERVAL: f64 = 1.0;
/// A sandbox with no radius wanders this far around its center.
const DEFAULT_SANDBOX_RADIUS: f32 = 512.0;
/// Combat movement stops within this distance of the selected target.
const COMBAT_ARRIVAL_DISTANCE: f32 = 4.0;
type PlannedPath = (Vec<[f32; 3]>, Vec<Option<GlobalFormId>>);

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const WKMV: Tag = Tag::new(b"WKMV");
const RNMV: Tag = Tag::new(b"RNMV");
const MTYP: Tag = Tag::new(b"MTYP");
const SPED: Tag = Tag::new(b"SPED");

/// What the chosen package makes the actor do.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Procedure {
    #[default]
    None,
    Travel,
    Sandbox,
    Sleep,
    Flee,
    Patrol,
    /// Hold an assigned location and react to detected actors within its radius.
    Guard,
    Sit,
    UseMagic,
    /// Issue one ordinary contact against the actor's current combat target.
    UseWeapon,
    /// A mental timer. The movement flag is retained for concurrent procedures.
    Wait {
        stop_movement: bool,
    },
    Hover,
    Follow,
    /// Escort a resolved target to the package branch location.
    Escort,
    /// Circle a resolved location inside the authored radii at a fixed height.
    Orbit {
        height: f32,
    },
    /// Approach the assigned target and start its assigned dialogue topic.
    ForceGreet,
    /// Approach the assigned actor and start a dialogue session in range.
    DialogueActivate,
    /// Walk to and hold an exclusive idle marker selected by the package.
    UseIdleMarker {
        marker: Option<GlobalFormId>,
    },
    /// Hold the position where this procedure branch began.
    HoldPosition,
    Shout,
    /// A procedure the driver does not run yet, by its template name. The
    /// actor holds where it stands.
    Unsupported(String),
    /// Approach and activate the assigned reference once.
    Activate,
    /// Select the nearest eligible reference matching the authored input.
    Find,
}

/// Per-actor driver state, saved with the actor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiState {
    pub package: Option<GlobalFormId>,
    /// The quest alias the package came from, none for a base package.
    pub alias: Option<(GlobalFormId, u32)>,
    pub procedure: Procedure,
    /// Saved position in the winning template's procedure tree.
    pub tree: Option<TreeState>,
    pub destination: Option<[f32; 3]>,
    /// Waypoints still ahead, first to reach first.
    pub path: Vec<[f32; 3]>,
    /// The cell the actor is in once it passes the matching waypoint.
    pub path_cells: Vec<Option<GlobalFormId>>,
    pub arrived: bool,
    /// A ForceGreet attempt was made for the current procedure branch.
    pub force_greet_started: bool,
    /// The Activate procedure has handed its one attempt to the game driver.
    pub activate_started: bool,
    /// DialogueActivate has opened its conversation for the current branch.
    pub dialogue_activate_started: bool,
    /// Reference selected by the active Find procedure.
    pub found_reference: Option<GlobalFormId>,
    pub sleeping: bool,
    pub bed: Option<GlobalFormId>,
    pub center: Option<[f32; 3]>,
    pub center_cell: Option<GlobalFormId>,
    pub radius: f32,
    /// Game hours until a sandboxing actor leaves its marker.
    pub dwell_until: f64,
    /// Finite Wait deadline in non-menu real seconds. None means endless Wait.
    pub wait_until: Option<f64>,
    /// Authored vertical offset while a Hover procedure is active.
    pub hover_height: f32,
    pub patrol: Option<PatrolState>,
    pub speed: f32,
    pub start_position: [f32; 3],
    pub reevaluate: bool,
    pub last_evaluation: f64,
    /// Packages that ran to completion, with the game day it happened on,
    /// for `Once Per Day`.
    pub completed: BTreeMap<GlobalFormId, u32>,
}

/// Per-world caches the driver builds on first use. Nothing here is saved,
/// all of it comes back from the load order.
#[derive(Default)]
pub struct AiWorld {
    nav: Option<NavGraph>,
    /// The cell each navmesh of `nav` sits in, by mesh index.
    nav_cells: Vec<Option<GlobalFormId>>,
    cell_refs: Option<HashMap<GlobalFormId, Vec<GlobalFormId>>>,
    /// Condition functions met during evaluation that have no
    /// implementation, with how often.
    pub unsupported_functions: BTreeMap<u16, usize>,
    /// Package evaluations run, for the performance harness.
    pub evaluations: usize,
}

impl AiWorld {
    pub fn nav_graph(&self) -> Option<&NavGraph> {
        self.nav.as_ref()
    }
}

/// Where a package's location input points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub position: [f32; 3],
    pub cell: Option<GlobalFormId>,
    pub reference: Option<GlobalFormId>,
}

/// A package and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub package: GlobalFormId,
    pub alias: Option<(GlobalFormId, u32)>,
}

// ----- views -----

/// The `PACK` view of a winning record.
pub fn package(world: &World, id: GlobalFormId) -> Option<Package> {
    let rec = world.record(id)?;
    let mut package = Package::from_record(rec).ok()?;
    if let Some(saved) = world.restored_pack_extra(id) {
        package.saved_waiting = saved.waiting;
        package.saved_never_run = saved.never_run;
    }
    Some(package)
}

pub(crate) fn movement_speed(world: &mut World, actor: GlobalFormId, preferred: u8) -> f32 {
    let (default_tag, offset, fallback) = movement_input(preferred);
    let Some(race) = world.actor(actor).and_then(|state| state.race) else {
        return fallback;
    };
    let Some(record) = world
        .record(race)
        .filter(|record| record.tag == T(b"RACE") && !record.is_deleted())
    else {
        return fallback;
    };
    let Some(raw) = record.find(default_tag).and_then(form_id) else {
        return fallback;
    };
    let Some(movement_type) = world.resolve_from(race, raw) else {
        return fallback;
    };
    let Some(movement_record) = world
        .record(movement_type)
        .filter(|record| record.tag == T(b"MOVT") && !record.is_deleted())
    else {
        return fallback;
    };
    race_override(world, race, record, movement_type, offset)
        .or_else(|| {
            movement_record
                .find(SPED)
                .and_then(|data| speed_at(data, offset))
        })
        .unwrap_or(fallback)
}

fn movement_input(preferred: u8) -> (Tag, usize, f32) {
    match preferred {
        speed::RUN => (RNMV, 20, RUN_SPEED),
        speed::JOG => (RNMV, 16, JOG_SPEED),
        speed::FAST_WALK => (WKMV, 20, FAST_WALK_SPEED),
        _ => (WKMV, 16, WALK_SPEED),
    }
}

fn race_override(
    world: &World,
    race: GlobalFormId,
    record: &soe_esm::Record,
    movement_type: GlobalFormId,
    offset: usize,
) -> Option<f32> {
    let mut current = None;
    for subrecord in &record.subrecords {
        if subrecord.tag == MTYP {
            current = form_id(&subrecord.data).and_then(|raw| world.resolve_from(race, raw));
        } else if subrecord.tag == SPED && current == Some(movement_type) {
            return speed_at(&subrecord.data, offset);
        }
    }
    None
}

fn form_id(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

fn speed_at(data: &[u8], offset: usize) -> Option<f32> {
    let speed = f32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
    (speed.is_finite() && speed > 0.0).then_some(speed)
}

/// The template chain of a package, innermost last: the package itself,
/// then its template, then that template's template, and so on.
fn template_chain(world: &World, id: GlobalFormId) -> Vec<(GlobalFormId, Package)> {
    let mut out = Vec::new();
    let mut next = Some(id);
    while let Some(cur) = next {
        if out.iter().any(|(seen, _)| *seen == cur) || out.len() > 8 {
            break;
        }
        let Some(p) = package(world, cur) else { break };
        next = p.template.and_then(|raw| world.resolve_from(cur, raw));
        out.push((cur, p));
    }
    out
}

/// The first procedure in tree order, without evaluating actor conditions.
/// Runtime execution uses the actor's saved tree cursor.
pub fn procedure_of(world: &World, id: GlobalFormId) -> (Procedure, Option<LocationData>, bool) {
    tree::first(world, id)
}

/// Ready ForceGreet attempts, resolved to the assigned target and topic.
pub(crate) fn force_greet_ready(
    world: &mut World,
) -> Vec<(GlobalFormId, GlobalFormId, GlobalFormId)> {
    let actors = world.loaded_actors();
    actors
        .into_iter()
        .filter_map(|actor| {
            let (ai, ghost) = {
                let state = world.actor(actor)?;
                (state.ai.clone(), state.ghost)
            };
            if ai.procedure != Procedure::ForceGreet || !ai.arrived || ai.force_greet_started {
                return None;
            }
            let alpha = world
                .reference(actor)
                .map_or(1.0, |reference| reference.alpha);
            if alpha < 1.0 && !ghost {
                if let Some(state) = world.actor(actor) {
                    state.ai.force_greet_started = true;
                }
                return None;
            }
            let ((owner, target), topic) = tree::force_greet_parameters(world, &ai)?;
            let target = resolve_follow_target(world, actor, &ai, (owner, target))?;
            world.actor(target)?;
            Some((actor, target, topic))
        })
        .collect()
}

pub(crate) fn force_greet_started(world: &mut World, actor: GlobalFormId) {
    if let Some(state) = world.actor(actor) {
        state.ai.force_greet_started = true;
    }
}

/// Ready Activate procedures, resolved to their assigned target. Attempt
/// state is marked before returning so a save or later tick cannot duplicate
/// the activation.
pub(crate) fn activate_ready(world: &mut World) -> Vec<(GlobalFormId, GlobalFormId)> {
    let actors = world.loaded_actors();
    actors
        .into_iter()
        .filter_map(|actor| {
            let ai = world.actor(actor)?.ai.clone();
            if ai.procedure != Procedure::Activate || !ai.arrived || ai.activate_started {
                return None;
            }
            let (owner, target) = tree::activate_target(world, &ai)?;
            let target = resolve_follow_target(world, actor, &ai, (owner, target))?;
            world.reference(target)?;
            world.actor(actor)?.ai.activate_started = true;
            Some((actor, target))
        })
        .collect()
}

/// DialogueActivate actors that are inside the configured dialogue range.
pub(crate) fn dialogue_activate_ready(world: &mut World) -> Vec<(GlobalFormId, GlobalFormId)> {
    if world.dialogue.session.is_some() {
        return Vec::new();
    }
    let Some(Value::Float(range)) = world.game_setting("fAIInDialogueModeWithPlayerDistance")
    else {
        return Vec::new();
    };
    if !range.is_finite() || range < 0.0 {
        return Vec::new();
    }
    let actors = world.loaded_actors();
    actors
        .into_iter()
        .filter_map(|actor| {
            let ai = world.actor(actor)?.ai.clone();
            if ai.procedure != Procedure::DialogueActivate || ai.dialogue_activate_started {
                return None;
            }
            let (owner, target) = tree::activate_target(world, &ai)?;
            let target = resolve_follow_target(world, actor, &ai, (owner, target))?;
            world.actor(target)?;
            let (actor_position, actor_cell) = {
                let reference = world.reference(actor)?;
                (reference.position, reference.cell)
            };
            let (target_position, target_cell) = {
                let reference = world.reference(target)?;
                (reference.position, reference.cell)
            };
            if actor_position
                .iter()
                .chain(&target_position)
                .any(|v| !v.is_finite())
            {
                return None;
            }
            let same_space = if actor_cell == target_cell {
                true
            } else {
                let worldspace = |cell: Option<GlobalFormId>| {
                    let cell = cell?;
                    if world.cell_is_interior(cell) {
                        return None;
                    }
                    let raw = world.record(cell)?.placement.world?;
                    world.resolve_from(cell, raw)
                };
                let actor_world = worldspace(actor_cell);
                actor_world.is_some() && actor_world == worldspace(target_cell)
            };
            let distance_squared: f64 = actor_position
                .iter()
                .zip(target_position)
                .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
                .sum();
            (same_space && distance_squared <= f64::from(range).powi(2)).then_some((actor, target))
        })
        .collect()
}

pub(crate) fn dialogue_activate_started(world: &mut World, actor: GlobalFormId) {
    if let Some(state) = world.actor(actor) {
        state.ai.dialogue_activate_started = true;
    }
}

// ----- the stack -----

/// The packages an actor could run, in evaluation order.
pub fn candidates(world: &mut World, actor: GlobalFormId) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut alias_packages: Vec<(u8, Candidate)> = Vec::new();
    for quest in world.loaded_quests() {
        let Some(q) = world.quest(quest) else {
            continue;
        };
        if !q.running {
            continue;
        }
        for alias in &q.def.aliases {
            if alias.packages.is_empty()
                || q.aliases.get(&alias.id).copied().flatten() != Some(actor)
            {
                continue;
            }
            for raw in &alias.packages {
                if let Some(package) = world.resolve_from(quest, *raw) {
                    alias_packages.push((
                        q.def.priority,
                        Candidate {
                            package,
                            alias: Some((quest, alias.id)),
                        },
                    ));
                }
            }
        }
    }
    alias_packages.sort_by_key(|a| std::cmp::Reverse(a.0));
    out.extend(alias_packages.into_iter().map(|(_, c)| c));
    let package_overrides = world
        .actor(actor)
        .map(|state| state.package_overrides.clone())
        .unwrap_or_default();
    if let Ok((source, npc)) = world.actor_npc_part(actor, crate::npc::template::AI_PACKAGES) {
        let mut packages: Vec<GlobalFormId> = npc
            .packages
            .iter()
            .filter_map(|raw| world.resolve_from(source, *raw))
            .collect();
        let mut overrides = package_overrides;
        overrides.sort_by_key(|(index, _)| *index);
        for (index, package) in overrides {
            let distributed = if world.tag_of(package) == Some(T(b"FLST")) {
                world.form_list(package).cloned().unwrap_or_default()
            } else {
                vec![package]
            };
            let mut at = index.min(packages.len());
            for package in distributed {
                if packages.contains(&package) {
                    continue;
                }
                packages.insert(at, package);
                at += 1;
            }
        }
        for package in packages {
            out.push(Candidate {
                package,
                alias: None,
            });
        }
    }
    out
}

/// The first candidate whose schedule and conditions hold. A running
/// `Must Complete` package that still holds keeps its place.
pub fn select(world: &mut World, actor: GlobalFormId) -> Option<Candidate> {
    world.ai.evaluations += 1;
    let today = world.clock.game_days.floor() as u32;
    let state = world.actor(actor)?;
    let current = state.ai.package;
    let completed = state.ai.completed.clone();
    let engaged = state.in_combat && state.combat_target.is_some();
    if let Some(current) = current {
        if let Some(current_package) = package(world, current) {
            let finished = world
                .actor(actor)
                .and_then(|state| state.ai.tree.as_ref())
                .is_some_and(|tree| tree.finished);
            let may_interrupt_for_combat = engaged
                && (current_package.general.interrupt_override == flags::INTERRUPT_OVERRIDE_COMBAT
                    || current_package.general.interrupt_flags & flags::INTERRUPT_OBSERVE_COMBAT
                        != 0);
            if current_package.has_flag(flags::MUST_COMPLETE)
                && !finished
                && !may_interrupt_for_combat
            {
                return Some(Candidate {
                    package: current,
                    alias: world.actor(actor).and_then(|state| state.ai.alias),
                });
            }
        }
    }
    let candidates = candidates(world, actor);
    for c in candidates {
        let Some(p) = package(world, c.package) else {
            continue;
        };
        if p.has_flag(flags::ONCE_PER_DAY) && completed.get(&c.package) == Some(&today) {
            continue;
        }
        if !schedule_allows_calendar(&p.schedule, &world.clock, world.calendar()) {
            continue;
        }
        let quest = c.alias.map(|(q, _)| q).or_else(|| {
            p.owner_quest
                .and_then(|raw| world.resolve_from(c.package, raw))
        });
        if !conditions_hold(world, actor, c.package, quest, &p.conditions) {
            continue;
        }
        return Some(c);
    }
    None
}

/// Whether the schedule window contains the clock's game time. Day of week
/// 0 is Sundas. Standalone clocks use the default start date. Live package
/// selection reads the date and hour from the world's calendar globals.
pub fn schedule_allows(s: &Schedule, clock: &GameClock) -> bool {
    schedule_allows_calendar(s, clock, crate::clock::Calendar::from_clock(clock))
}

fn schedule_allows_calendar(
    s: &Schedule,
    clock: &GameClock,
    calendar: crate::clock::Calendar,
) -> bool {
    let day = clock.game_days.floor() as i64;
    if s.day_of_week >= 0 {
        let dow = (day.rem_euclid(7)) as i8;
        let ok = match s.day_of_week {
            0..=6 => dow == s.day_of_week,
            7 => (1..=5).contains(&dow),
            8 => dow == 0 || dow == 6,
            9 => matches!(dow, 1 | 3 | 5),
            10 => matches!(dow, 2 | 4),
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    if s.date > 0 {
        let dom = calendar.day as i8;
        if dom != s.date {
            return false;
        }
    }
    if s.month >= 0 {
        let month = calendar.month as i8;
        if month != s.month {
            return false;
        }
    }
    if s.hour < 0 {
        return true;
    }
    let start = f64::from(s.hour) * 60.0 + f64::from(s.minute.max(0));
    let now = calendar.hour * 60.0;
    if s.duration_minutes == 0 {
        return now >= start;
    }
    let end = start + f64::from(s.duration_minutes);
    if end <= 1440.0 {
        now >= start && now < end
    } else {
        now >= start || now < end - 1440.0
    }
}

/// Evaluate a condition list the way the game does: conditions are
/// combined with AND, except that a condition flagged OR joins the one
/// after it into a group that passes when any member does.
pub fn conditions_hold(
    world: &mut World,
    subject: GlobalFormId,
    package_id: GlobalFormId,
    quest: Option<GlobalFormId>,
    conditions: &[Condition],
) -> bool {
    let mut i = 0;
    while i < conditions.len() {
        let mut any = false;
        loop {
            let c = &conditions[i];
            if condition_holds(world, subject, package_id, quest, c) {
                any = true;
            }
            i += 1;
            if !c.is_or() || i >= conditions.len() {
                break;
            }
        }
        if !any {
            return false;
        }
    }
    true
}

fn condition_holds(
    world: &mut World,
    subject: GlobalFormId,
    package_id: GlobalFormId,
    quest: Option<GlobalFormId>,
    c: &Condition,
) -> bool {
    let target = match c.run_on {
        0 => Some(subject),
        2 => world.resolve_from(package_id, c.reference),
        5 => quest.and_then(|q| {
            u32::try_from(c.param3)
                .ok()
                .and_then(|alias| world.alias_ref(q, alias))
        }),
        _ => None,
    };
    let Some(target) = target else {
        *world
            .ai
            .unsupported_functions
            .entry(c.function)
            .or_insert(0) += 1;
        return false;
    };
    let Some(actual) = function_value(world, target, package_id, quest, c) else {
        *world
            .ai
            .unsupported_functions
            .entry(c.function)
            .or_insert(0) += 1;
        return false;
    };
    let expected = match c.value {
        CompValue::Float(v) => v,
        CompValue::Global(raw) => world
            .resolve_from(package_id, raw)
            .map_or(0.0, |g| world.global_value(g)),
    };
    match c.comparison {
        condition::EQUAL => (actual - expected).abs() < 1e-4,
        condition::NOT_EQUAL => (actual - expected).abs() >= 1e-4,
        condition::GREATER => actual > expected,
        condition::GREATER_OR_EQUAL => actual >= expected,
        condition::LESS => actual < expected,
        condition::LESS_OR_EQUAL => actual <= expected,
        _ => false,
    }
}

fn bool_f(v: bool) -> f32 {
    if v {
        1.0
    } else {
        0.0
    }
}

fn cell_of(world: &mut World, id: GlobalFormId) -> Option<GlobalFormId> {
    world.reference(id).and_then(|r| r.cell)
}

/// The value of a condition function on `subject`, none for a function the
/// engine cannot answer yet.
fn function_value(
    world: &mut World,
    subject: GlobalFormId,
    package_id: GlobalFormId,
    quest: Option<GlobalFormId>,
    c: &Condition,
) -> Option<f32> {
    let form = |world: &World, raw: u32| world.resolve_from(package_id, raw);
    Some(match c.function {
        1 => {
            let r = form(world, c.param1)?;
            world.distance(subject, r)
        }
        14 | 128 | 277 | 408 | 430 | 494 | 580 | 694 => {
            crate::conditions::actor_query(world, package_id, subject, c.function, c.param1).ok()?
        }
        18 => world.calendar().hour as f32,
        32 => {
            let r = form(world, c.param1)?;
            let a = cell_of(world, subject);
            bool_f(a.is_some() && a == cell_of(world, r))
        }
        46 => bool_f(world.actor(subject).is_some_and(|a| a.dead)),
        47 => {
            let item = form(world, c.param1)?;
            world.item_count(subject, item) as f32
        }
        49 => {
            if world.actor(subject).is_some_and(|a| a.ai.sleeping) {
                3.0
            } else {
                0.0
            }
        }
        56 => {
            let q = form(world, c.param1)?;
            bool_f(world.quest_state(q).is_some_and(|q| q.running))
        }
        58 => {
            let q = form(world, c.param1)?;
            world.quest_state(q).map_or(0.0, |q| f32::from(q.highest))
        }
        59 => {
            let q = form(world, c.param1)?;
            let stage = c.param2 as u16;
            bool_f(
                world
                    .quest_state(q)
                    .is_some_and(|q| q.done.contains(&stage)),
            )
        }
        67 => {
            let cell = form(world, c.param1)?;
            bool_f(cell_of(world, subject) == Some(cell))
        }
        69 => {
            let race = form(world, c.param1)?;
            bool_f(world.actor(subject).is_some_and(|a| a.race == Some(race)))
        }
        70 => {
            let female = world
                .actor_npc_part(subject, crate::npc::template::TRAITS)
                .is_ok_and(|(_, npc)| npc.is_female());
            bool_f(female == (c.param1 == 1))
        }
        71 => {
            let faction = form(world, c.param1)?;
            bool_f(
                world
                    .actor(subject)
                    .is_some_and(|a| a.factions.contains_key(&faction)),
            )
        }
        72 => {
            let base = form(world, c.param1)?;
            bool_f(world.base_of(subject) == Some(base))
        }
        73 => {
            let faction = form(world, c.param1)?;
            world
                .actor(subject)
                .and_then(|a| a.factions.get(&faction).copied())
                .map_or(-1.0, |r| r as f32)
        }
        74 => {
            let g = form(world, c.param1)?;
            world.global_value(g)
        }
        77 => world.random_int(0, 99) as f32,
        80 => world.actor(subject).map_or(0.0, |a| a.level as f32),
        98 => bool_f(!world.player_controls),
        136 => {
            let r = form(world, c.param1)?;
            bool_f(subject == r)
        }
        161 => {
            let p = form(world, c.param1)?;
            bool_f(
                world
                    .actor(subject)
                    .is_some_and(|a| a.ai.package == Some(p)),
            )
        }
        170 => (world.clock.game_days.floor() as i64).rem_euclid(7) as f32,
        286 => 0.0,
        289 => bool_f(world.actor(subject).is_some_and(|a| a.in_combat)),
        300 => {
            let cell = cell_of(world, subject)?;
            bool_f(world.cell_is_interior(cell))
        }
        543 => {
            let q = form(world, c.param1)?;
            bool_f(world.quest_state(q).is_some_and(|q| q.completed))
        }
        560 => {
            let k = form(world, c.param1)?;
            bool_f(world.has_keyword(subject, k))
        }
        566 => {
            let q = quest?;
            bool_f(world.alias_ref(q, c.param1) == Some(subject))
        }
        640 => {
            let name = actor_value_name(i32::try_from(c.param1).ok()?)?;
            let base = world.base_actor_value(subject, name);
            if base > 0.0 {
                world.actor_value(subject, name) / base
            } else {
                0.0
            }
        }
        _ => return None,
    })
}

// ----- switching packages -----

/// Re-evaluate the stack now and switch when the winner changed. Queues
/// `OnPackageEnd`, `OnPackageChange` and `OnPackageStart` for the actor's
/// scripts. Returns the package now running.
pub fn evaluate(world: &mut World, actor: GlobalFormId, alloc: Alloc<'_>) -> Option<GlobalFormId> {
    let chosen = select(world, actor);
    let now = world.clock.real_seconds;
    let current = world.actor(actor)?.ai.package;
    if let Some(a) = world.actor(actor) {
        a.ai.reevaluate = false;
        a.ai.last_evaluation = now;
    }
    if chosen.map(|c| c.package) == current {
        return current;
    }
    if let Some(old) = current {
        if package(world, old).is_some_and(|p| p.has_flag(flags::UNLOCK_DOORS_ON_CHANGE)) {
            unlock_cell_doors(world, actor, alloc);
        }
    }
    let object = world.object_of(Handle::Form(actor), alloc);
    if let Some(old) = current {
        let old_value = world.form_value(Some(old), alloc);
        let name = if chosen.is_some() {
            queue_pack_fragment(world, old, actor, PackBoundary::Change, alloc);
            if let Some(p) = package(world, old) {
                start_boundary_action(world, old, actor, p.on_change, alloc);
            }
            "OnPackageChange"
        } else {
            queue_pack_fragment(world, old, actor, PackBoundary::End, alloc);
            if let Some(p) = package(world, old) {
                start_boundary_action(world, old, actor, p.on_end, alloc);
            }
            "OnPackageEnd"
        };
        world.pending.push(Pending::Event {
            object,
            name: name.into(),
            args: vec![old_value],
        });
    }
    let position = world
        .reference(actor)
        .map(|r| r.position)
        .unwrap_or_default();
    let mut ai = std::mem::take(&mut world.actor(actor)?.ai);
    if ai.procedure == Procedure::Sit {
        world.stand_from_furniture(actor);
    }
    ai.package = chosen.map(|c| c.package);
    ai.alias = chosen.and_then(|c| c.alias);
    ai.procedure = Procedure::None;
    ai.tree = None;
    ai.destination = None;
    ai.path.clear();
    ai.path_cells.clear();
    ai.arrived = false;
    ai.force_greet_started = false;
    ai.activate_started = false;
    ai.dialogue_activate_started = false;
    ai.found_reference = None;
    ai.sleeping = false;
    ai.bed = None;
    ai.center = None;
    ai.center_cell = None;
    ai.radius = 0.0;
    ai.dwell_until = 0.0;
    ai.wait_until = None;
    ai.hover_height = 0.0;
    ai.patrol = None;
    ai.speed = movement_speed(world, actor, speed::WALK);
    ai.start_position = position;
    if let Some(c) = chosen {
        queue_pack_fragment(world, c.package, actor, PackBoundary::Begin, alloc);
        if let Some(p) = package(world, c.package) {
            start_boundary_action(world, c.package, actor, p.on_begin, alloc);
            ai.speed = movement_speed(
                world,
                actor,
                if p.has_flag(flags::PREFERRED_SPEED) {
                    p.general.preferred_speed
                } else {
                    speed::WALK
                },
            );
            if p.has_flag(flags::UNLOCK_DOORS_AT_START) {
                unlock_cell_doors(world, actor, alloc);
            }
        }
        tree::start(world, actor, &mut ai, &mut *alloc);
        let new_value = world.form_value(Some(c.package), alloc);
        world.pending.push(Pending::Event {
            object,
            name: "OnPackageStart".into(),
            args: vec![new_value],
        });
    }
    let running = ai.package;
    if let Some(a) = world.actor(actor) {
        a.ai = ai;
    }
    running
}

fn unlock_cell_doors(world: &mut World, actor: GlobalFormId, alloc: Alloc<'_>) {
    let Some(cell) = world.reference(actor).and_then(|reference| reference.cell) else {
        return;
    };
    let owner_base = world.base_of(actor);
    let doors = world.cell_doors(cell);
    for (door, _) in doors {
        let Some((owner, locked)) = world
            .reference(door)
            .map(|reference| (reference.owner, reference.locked))
        else {
            continue;
        };
        let owned_by_actor = owner.is_some_and(|owner| {
            owner_base == Some(owner)
                || world
                    .actor(actor)
                    .is_some_and(|state| state.factions.contains_key(&owner))
        });
        if locked && owned_by_actor {
            world.set_locked(door, false, alloc);
        }
    }
}

fn start_boundary_action(
    world: &mut World,
    package_id: GlobalFormId,
    actor: GlobalFormId,
    action: crate::package::Action,
    alloc: Alloc<'_>,
) {
    if world
        .actor(actor)
        .is_none_or(|state| state.dead || state.unconscious)
    {
        return;
    }

    if let Some(raw_idle) = action.idle {
        if let Some(idle) = world.resolve_from(package_id, raw_idle) {
            if let Some(name) = world.editor_id(idle) {
                world.animation_event(actor, &name, alloc);
            }
        }
    }

    let Some(crate::package::Topic::Dialogue(raw_topic)) = action.topic else {
        return;
    };
    let Some(topic) = world.resolve_from(package_id, raw_topic) else {
        return;
    };
    let listener = world.player();
    if listener == actor
        || world.dialogue.session.is_some()
        || world.actor(actor).is_none_or(|state| {
            state.dead || state.in_combat || state.unconscious || !state.allow_pc_dialogue
        })
        || world.actor(listener).is_none_or(|state| state.dead)
    {
        return;
    }
    world.dialogue.greeting_actor = None;
    world.dialogue.session = Some(crate::dialogue::Session {
        speaker: actor,
        listener,
        topic: Some(topic),
        current_info: None,
        last_info: None,
        response_index: 0,
    });
}

#[derive(Clone, Copy)]
enum PackBoundary {
    Begin,
    End,
    Change,
}

fn queue_pack_fragment(
    world: &mut World,
    package: GlobalFormId,
    actor: GlobalFormId,
    boundary: PackBoundary,
    alloc: Alloc<'_>,
) {
    let fragment = world
        .record(package)
        .and_then(|record| record.find(Tag::new(b"VMAD")))
        .and_then(|raw| Vmad::parse(raw, Tag::new(b"PACK")).ok())
        .and_then(|vmad| vmad.pack_fragments)
        .and_then(|fragments| match boundary {
            PackBoundary::Begin => fragments.begin,
            PackBoundary::End => fragments.end,
            PackBoundary::Change => fragments.change,
        });
    let Some(fragment) = fragment else {
        return;
    };
    let object = world.object_of(Handle::Form(package), alloc);
    let actor = Value::Object(world.object_of(Handle::Form(actor), alloc));
    world.pending.push(Pending::CallScript {
        object,
        script: fragment.script,
        function: fragment.function,
        args: vec![actor],
    });
}

fn begin_procedure(
    world: &mut World,
    actor: GlobalFormId,
    ai: &mut AiState,
    procedure: Procedure,
    location: Option<(GlobalFormId, LocationData)>,
    hover_height: Option<f32>,
) {
    let (position, cell) = world
        .reference(actor)
        .map(|r| (r.position, r.cell))
        .unwrap_or_default();
    if ai.procedure == Procedure::Sit {
        world.stand_from_furniture(actor);
    }
    ai.procedure = procedure;
    ai.destination = None;
    ai.path.clear();
    ai.path_cells.clear();
    ai.arrived = false;
    ai.force_greet_started = false;
    ai.activate_started = false;
    ai.dialogue_activate_started = false;
    ai.found_reference = None;
    ai.sleeping = false;
    ai.bed = None;
    ai.center = None;
    ai.center_cell = None;
    ai.radius = 0.0;
    ai.dwell_until = 0.0;
    ai.wait_until = None;
    ai.hover_height = 0.0;
    ai.patrol = None;
    let alias = ai.alias.or_else(|| {
        let id = ai.package?;
        let raw = package(world, id)?.owner_quest?;
        Some((world.resolve_from(id, raw)?, 0))
    });
    let place =
        location.and_then(|(owner, l)| resolve_location(world, actor, owner, alias, ai, &l));
    if let Some(height) = hover_height {
        ai.hover_height = height;
    }
    match ai.procedure {
        Procedure::Travel => {
            ai.destination = place.map(|p| p.position);
            ai.center_cell = place.and_then(|p| p.cell);
        }
        Procedure::Sandbox => {
            let center = place.map_or(position, |p| p.position);
            ai.center = Some(center);
            ai.center_cell = place.and_then(|p| p.cell).or(cell);
            ai.radius = location.map_or(0.0, |(_, l)| l.radius as f32);
            if ai.radius <= 0.0 {
                ai.radius = DEFAULT_SANDBOX_RADIUS;
            }
        }
        Procedure::Sleep => {
            let bed = place
                .and_then(|p| p.reference)
                .filter(|r| world.tag_of(*r) == Some(T(b"FURN")) || is_bed(world, *r))
                .or_else(|| {
                    let search_cell = place.and_then(|p| p.cell).or(cell)?;
                    let near = place.map_or(position, |p| p.position);
                    nearest_bed(world, search_cell, near)
                });
            ai.bed = bed;
            ai.destination = bed
                .and_then(|b| world.reference(b).map(|r| r.position))
                .or_else(|| place.map(|p| p.position));
            ai.center_cell = bed
                .and_then(|b| world.reference(b).and_then(|r| r.cell))
                .or_else(|| place.and_then(|p| p.cell));
        }
        Procedure::Flee => ai.speed = movement_speed(world, actor, speed::RUN),
        Procedure::Guard => {
            let center = place.map_or(position, |p| p.position);
            ai.center = Some(center);
            ai.center_cell = place.and_then(|p| p.cell).or(cell);
            ai.radius = location.map_or(0.0, |(_, l)| l.radius.max(0) as f32);
            if distance(position, center) > ai.radius.max(ARRIVAL_DISTANCE) {
                ai.destination = Some(center);
            } else {
                ai.arrived = true;
            }
        }
        Procedure::Hover => {
            if let Some(place) = place {
                ai.destination = Some([
                    place.position[0],
                    place.position[1],
                    place.position[2] + ai.hover_height,
                ]);
                ai.center_cell = place.cell;
            } else {
                ai.procedure = Procedure::Unsupported("Hover location, AI-31".into());
            }
        }
        Procedure::Follow => {
            if let Some((owner, target, _, _)) = tree::follow_parameters(world, ai) {
                if resolve_follow_target(world, actor, ai, (owner, target)).is_none() {
                    ai.procedure = Procedure::Unsupported("Follow target".into());
                }
            } else {
                ai.procedure = Procedure::Unsupported("Follow inputs".into());
            }
        }
        Procedure::Escort => {
            match tree::escort_target(world, ai).and_then(|(owner, target, radius)| {
                let target = resolve_follow_target(world, actor, ai, (owner, target))?;
                world.reference(target).map(|_| (target, radius))
            }) {
                Some((_, radius)) if radius < 0 => {
                    ai.procedure = Procedure::Unsupported("Escort distance".into());
                }
                Some((_, radius)) => match place {
                    Some(place) => {
                        ai.destination = Some(place.position);
                        ai.center = Some(place.position);
                        ai.center_cell = place.cell;
                        ai.radius = radius as f32;
                    }
                    None => ai.procedure = Procedure::Unsupported("Escort location".into()),
                },
                None => ai.procedure = Procedure::Unsupported("Escort target".into()),
            }
        }
        Procedure::ForceGreet => {
            if let Some((target, _)) = tree::force_greet_parameters(world, ai) {
                if let Some(target) = resolve_follow_target(world, actor, ai, target) {
                    if let Some(target_ref) = world.reference(target) {
                        ai.destination = Some(target_ref.position);
                        ai.center_cell = target_ref.cell;
                    } else {
                        ai.procedure =
                            Procedure::Unsupported("ForceGreet target is not placed".into());
                    }
                } else {
                    ai.procedure = Procedure::Unsupported("ForceGreet target".into());
                }
            } else {
                ai.procedure = Procedure::Unsupported("ForceGreet inputs".into());
            }
        }
        Procedure::Activate => {
            if let Some((owner, target)) = tree::activate_target(world, ai) {
                if let Some(target) = resolve_follow_target(world, actor, ai, (owner, target)) {
                    if let Some(target_ref) = world.reference(target) {
                        ai.destination = Some(target_ref.position);
                        ai.center_cell = target_ref.cell;
                    } else {
                        ai.procedure =
                            Procedure::Unsupported("Activate target is not placed".into());
                    }
                } else {
                    ai.procedure = Procedure::Unsupported("Activate target".into());
                }
            } else {
                ai.procedure = Procedure::Unsupported("Activate inputs".into());
            }
        }
        Procedure::DialogueActivate => {
            if let Some((owner, target)) = tree::activate_target(world, ai) {
                if let Some(target) = resolve_follow_target(world, actor, ai, (owner, target)) {
                    if let Some(target_ref) = world.reference(target) {
                        ai.destination = Some(target_ref.position);
                        ai.center_cell = target_ref.cell;
                    } else {
                        ai.procedure =
                            Procedure::Unsupported("DialogueActivate target is not placed".into());
                    }
                } else {
                    ai.procedure = Procedure::Unsupported("DialogueActivate target".into());
                }
            } else {
                ai.procedure = Procedure::Unsupported("DialogueActivate inputs".into());
            }
        }
        Procedure::Find => {
            match tree::find_target(world, ai).and_then(|(owner, target, location)| {
                let place = match location {
                    Some((owner, location)) => Some(resolve_location(
                        world, actor, owner, ai.alias, ai, &location,
                    )?),
                    None => None,
                };
                find_reference(
                    world,
                    actor,
                    owner,
                    target,
                    place,
                    location.map(|(_, l)| l.radius),
                )
            }) {
                Some(reference) => ai.found_reference = Some(reference),
                None => ai.procedure = Procedure::Unsupported("Find inputs or reference".into()),
            }
        }
        Procedure::UseIdleMarker { .. } => match (place, location) {
            (Some(place), Some((_, location))) => {
                ai.center = Some(place.position);
                ai.center_cell = place.cell.or(cell);
                ai.radius = location.radius as f32;
                if !ai.radius.is_finite() || ai.radius < 0.0 {
                    ai.procedure = Procedure::Unsupported("UseIdleMarker radius".into());
                }
            }
            _ => ai.procedure = Procedure::Unsupported("UseIdleMarker location".into()),
        },
        Procedure::HoldPosition => {
            ai.center = Some(place.map_or(position, |place| place.position));
            ai.center_cell = place.and_then(|place| place.cell).or(cell);
        }
        Procedure::Sit => {
            let furniture = place.and_then(|p| p.reference).filter(|reference| {
                world
                    .base_of(*reference)
                    .and_then(|base| world.tag_of(base))
                    == Some(T(b"FURN"))
            });
            if let Some(furniture) = furniture
                .filter(|furniture| world.reserve_furniture_marker(*furniture, actor).is_some())
            {
                ai.destination = world
                    .reference(furniture)
                    .map(|reference| reference.position);
                ai.center_cell = world
                    .reference(furniture)
                    .and_then(|reference| reference.cell);
            } else {
                ai.procedure = Procedure::Unsupported("Sit furniture unavailable".into());
            }
        }
        Procedure::None
        | Procedure::Wait { .. }
        | Procedure::Shout
        | Procedure::Patrol
        | Procedure::Orbit { .. }
        | Procedure::UseMagic
        | Procedure::UseWeapon
        | Procedure::Unsupported(_) => {}
    }
}

fn find_reference(
    world: &mut World,
    actor: GlobalFormId,
    owner: GlobalFormId,
    target: crate::package::Target,
    place: Option<Place>,
    radius: Option<i32>,
) -> Option<GlobalFormId> {
    use crate::package::Target;

    let (base, object_type) = match target {
        Target::ObjectId(raw) => (Some(world.resolve_from(owner, raw)?), None),
        Target::ObjectType(kind @ 1..=15) => (None, Some(kind)),
        _ => return None,
    };
    let actor_cell = world.reference(actor)?.cell;
    let cell = place.and_then(|place| place.cell).or(actor_cell)?;
    let center = place.map_or(world.reference(actor)?.position, |place| place.position);
    let radius = match radius {
        Some(radius) if radius < 0 => return None,
        Some(radius) => Some(radius as f32),
        None => None,
    };
    references_in_cell(world, cell)
        .into_iter()
        .filter(|reference| *reference != actor)
        .filter_map(|reference| {
            let (position, eligible) = world.reference(reference).map(|state| {
                (
                    state.position,
                    !state.deleted && !state.destroyed && state.enabled,
                )
            })?;
            if !eligible || position.iter().any(|value| !value.is_finite()) {
                return None;
            }
            let candidate_base = world.base_of(reference)?;
            let matches = base.is_some_and(|base| base == candidate_base)
                || object_type
                    .is_some_and(|kind| matches_package_object_type(world, candidate_base, kind));
            if !matches || radius.is_some_and(|radius| distance(center, position) > radius) {
                return None;
            }
            Some((distance(center, position), reference))
        })
        .min_by(|(left_distance, left_id), (right_distance, right_id)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| left_id.cmp(right_id))
        })
        .map(|(_, reference)| reference)
}

fn matches_package_object_type(world: &World, base: GlobalFormId, kind: u32) -> bool {
    let tag = world.tag_of(base);
    let expected = match kind {
        1 => T(b"ACTI"),
        2 => T(b"ARMO"),
        3 => T(b"BOOK"),
        4 => T(b"CONT"),
        5 => T(b"DOOR"),
        6 => T(b"INGR"),
        7 => T(b"LIGH"),
        8 => T(b"MISC"),
        9 => T(b"FLOR"),
        10 => T(b"FURN"),
        11 => T(b"WEAP"),
        12 => T(b"AMMO"),
        13 => T(b"KEYM"),
        14 | 15 => T(b"ALCH"),
        _ => return false,
    };
    if tag != Some(expected) {
        return false;
    }
    if matches!(kind, 14 | 15) {
        let food = world
            .record(base)
            .and_then(|record| record.find(T(b"ENIT")))
            .and_then(|data| data.get(4..8))
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .is_some_and(|bytes| u32::from_le_bytes(bytes) & 2 != 0);
        return food == (kind == 15);
    }
    true
}

fn resolve_follow_target(
    world: &mut World,
    actor: GlobalFormId,
    ai: &AiState,
    target: (GlobalFormId, crate::package::Target),
) -> Option<GlobalFormId> {
    use crate::package::Target;

    let (owner, target) = target;
    match target {
        Target::Reference(raw) if raw != 0 => world.resolve_from(owner, raw),
        Target::Alias(alias) => {
            let quest = ai.alias.map(|(quest, _)| quest).or_else(|| {
                package(world, ai.package?)?
                    .owner_quest
                    .and_then(|raw| world.resolve_from(ai.package?, raw))
            })?;
            world.alias_ref(quest, alias)
        }
        Target::LinkedReference(keyword) => {
            if keyword == 0 {
                world
                    .reference(actor)?
                    .linked
                    .first()
                    .map(|(_, reference)| *reference)
            } else {
                let keyword = world.resolve_from(owner, keyword)?;
                world
                    .reference(actor)?
                    .linked
                    .iter()
                    .find(|(key, _)| *key == Some(keyword))
                    .map(|(_, reference)| *reference)
            }
        }
        Target::Itself => Some(actor),
        _ => None,
    }
}

fn escort_target_arrived(world: &mut World, actor: GlobalFormId, ai: &AiState) -> bool {
    tree::escort_target(world, ai)
        .and_then(|(owner, target, _)| resolve_follow_target(world, actor, ai, (owner, target)))
        .and_then(|target| world.reference(target).map(|reference| reference.position))
        .is_some_and(|target| {
            distance(target, ai.destination.unwrap_or_default()) <= ARRIVAL_DISTANCE
        })
}

fn escort_can_advance(
    world: &mut World,
    actor: GlobalFormId,
    ai: &AiState,
    position: [f32; 3],
) -> bool {
    let Some(goal) = ai.center else {
        return false;
    };
    let target = tree::escort_target(world, ai)
        .and_then(|(owner, target, _)| resolve_follow_target(world, actor, ai, (owner, target)))
        .and_then(|target| world.reference(target).map(|reference| reference.position));
    target.is_some_and(|target| {
        distance(position, target) <= ai.radius || distance(target, goal) < distance(position, goal)
    })
}

/// Where a package location input points for this actor.
pub fn resolve_location(
    world: &mut World,
    actor: GlobalFormId,
    package_id: GlobalFormId,
    alias: Option<(GlobalFormId, u32)>,
    ai: &AiState,
    l: &LocationData,
) -> Option<Place> {
    let at_ref = |world: &mut World, r: GlobalFormId| {
        world.reference(r).map(|s| Place {
            position: s.position,
            cell: s.cell,
            reference: Some(r),
        })
    };
    match l.location {
        Location::NearReference(raw) => {
            let r = world.resolve_from(package_id, raw)?;
            at_ref(world, r)
        }
        Location::InCell(raw) => {
            let cell = world.resolve_from(package_id, raw)?;
            let first = references_in_cell(world, cell).into_iter().next();
            let position = first
                .and_then(|r| world.reference(r).map(|s| s.position))
                .unwrap_or_default();
            Some(Place {
                position,
                cell: Some(cell),
                reference: None,
            })
        }
        Location::NearPackageStart => Some(Place {
            position: ai.start_position,
            cell: cell_of(world, actor),
            reference: None,
        }),
        Location::NearEditorLocation => {
            let position = world.editor_position(actor)?;
            Some(Place {
                position,
                cell: cell_of(world, actor),
                reference: None,
            })
        }
        Location::NearLinkedReference(raw) => {
            let keyword = (raw != 0)
                .then(|| world.resolve_from(package_id, raw))
                .flatten();
            let linked = world.reference(actor)?.linked.clone();
            let target = linked
                .iter()
                .find(|(k, _)| keyword.is_none() || *k == keyword)
                .map(|(_, r)| *r)?;
            at_ref(world, target)
        }
        Location::AliasReference(id) => {
            let quest = alias.map(|(q, _)| q).or_else(|| {
                package(world, package_id)
                    .and_then(|p| p.owner_quest)
                    .and_then(|raw| world.resolve_from(package_id, raw))
            })?;
            let r = world.alias_ref(quest, id)?;
            at_ref(world, r)
        }
        Location::NearSelf => Some(Place {
            position: world.reference(actor)?.position,
            cell: cell_of(world, actor),
            reference: None,
        }),
        Location::AliasLocation(id) => {
            let quest = alias.map(|(q, _)| q).or_else(|| {
                package(world, package_id)
                    .and_then(|p| p.owner_quest)
                    .and_then(|raw| world.resolve_from(package_id, raw))
            })?;
            if !world.alias_is_location(quest, id) {
                return None;
            }
            let location = world.alias_ref(quest, id)?;
            let marker = world.location_marker(location)?;
            if world.reference(marker)?.deleted {
                return None;
            }
            at_ref(world, marker)
        }
        Location::Other(..) => None,
    }
}

fn is_bed(world: &mut World, r: GlobalFormId) -> bool {
    let Some(base) = world.base_of(r) else {
        return false;
    };
    if world.tag_of(base) != Some(T(b"FURN")) {
        return false;
    }
    world.keywords(base).iter().any(|k| {
        world
            .editor_id(*k)
            .is_some_and(|e| e.to_ascii_lowercase().contains("bed"))
    })
}

fn nearest_bed(world: &mut World, cell: GlobalFormId, near: [f32; 3]) -> Option<GlobalFormId> {
    let mut best: Option<(f32, GlobalFormId)> = None;
    for r in references_in_cell(world, cell) {
        if !is_bed(world, r) {
            continue;
        }
        let Some(p) = world.reference(r).map(|s| s.position) else {
            continue;
        };
        let d = distance(p, near);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, r));
        }
    }
    best.map(|(_, r)| r)
}

/// Idle markers and furniture (beds excluded) within `radius` of `center`
/// in `cell`.
fn sandbox_markers(
    world: &mut World,
    cell: GlobalFormId,
    center: [f32; 3],
    radius: f32,
) -> Vec<GlobalFormId> {
    let mut out = Vec::new();
    for r in references_in_cell(world, cell) {
        let Some(base) = world.base_of(r) else {
            continue;
        };
        let tag = world.tag_of(base);
        if tag != Some(T(b"IDLM")) && tag != Some(T(b"FURN")) {
            continue;
        }
        if tag == Some(T(b"FURN")) && is_bed(world, r) {
            continue;
        }
        let Some(p) = world.reference(r).map(|s| s.position) else {
            continue;
        };
        if distance(p, center) <= radius {
            out.push(r);
        }
    }
    out
}

/// Idle markers in the package location that no other actor's active
/// UseIdleMarker procedure currently holds.
fn idle_markers(
    world: &mut World,
    actor: GlobalFormId,
    cell: GlobalFormId,
    center: [f32; 3],
    radius: f32,
) -> Vec<GlobalFormId> {
    let refs = references_in_cell(world, cell);
    let mut eligible = Vec::new();
    for reference in refs.iter().copied() {
        if world
            .base_of(reference)
            .is_none_or(|base| world.tag_of(base) != Some(T(b"IDLM")))
        {
            continue;
        }
        if !world.reference(reference).is_some_and(|state| {
            state.enabled
                && !state.deleted
                && !state.destroyed
                && distance(state.position, center) <= radius
        }) {
            continue;
        }
        let reserved = refs.iter().copied().any(|other| {
            other != actor
                && world.actor(other).is_some_and(|state| {
                    matches!(state.ai.procedure, Procedure::UseIdleMarker { marker: Some(held) } if held == reference)
                })
        });
        if !reserved {
            eligible.push(reference);
        }
    }
    eligible
}

/// Every placed reference and actor in a cell, from an index built once
/// over the load order.
pub fn references_in_cell(world: &mut World, cell: GlobalFormId) -> Vec<GlobalFormId> {
    if world.ai.cell_refs.is_none() {
        let _span = soe_trace::Span::new("ai.cell_index", "game");
        let mut index: HashMap<GlobalFormId, Vec<GlobalFormId>> = HashMap::new();
        let order = world.order();
        for tag in [T(b"REFR"), T(b"ACHR")] {
            for (id, _) in order.winners_of(tag) {
                let Some(rref) = order.winner_ref(id) else {
                    continue;
                };
                let rec = order.record(rref);
                let Some(raw_cell) = rec.placement.cell else {
                    continue;
                };
                let cell = order.resolve(rref.file, raw_cell);
                index.entry(cell).or_default().push(id);
            }
        }
        for refs in index.values_mut() {
            refs.sort();
        }
        world.ai.cell_refs = Some(index);
    }
    world
        .ai
        .cell_refs
        .as_ref()
        .and_then(|i| i.get(&cell))
        .cloned()
        .unwrap_or_default()
}

/// The navmesh graph over every winning `NAVM`, built on first use.
pub fn nav_graph(world: &mut World) -> &NavGraph {
    ensure_nav_graph(world);
    world.ai.nav.get_or_insert_with(NavGraph::default)
}

fn ensure_nav_graph(world: &mut World) {
    if world.ai.nav.is_none() {
        let _span = soe_trace::Span::new("ai.nav_graph", "game");
        let order = world.order();
        let mut b = Builder::new();
        let mut cells = Vec::new();
        let mut winners: Vec<_> = order
            .winners_of(T(b"NAVM"))
            .filter_map(|(id, _)| order.winner_ref(id).map(|r| (id, r)))
            .collect();
        winners.sort_by_key(|(id, _)| *id);
        for (id, rref) in winners {
            let rec = order.record(rref);
            let Ok(navm) = NavMesh::from_record(rec) else {
                continue;
            };
            let Some(geometry) = navm.geometry else {
                continue;
            };
            let file = rref.file;
            b.add(key_of(id), &geometry, |raw| {
                Some(key_of(order.resolve(file, raw)))
            });
            cells.push(rec.placement.cell.map(|c| order.resolve(file, c)));
        }
        let (graph, _) = b.build();
        world.ai.nav = Some(graph);
        world.ai.nav_cells = cells;
    }
}

// ----- the driver -----

/// Advance every loaded actor: evaluate when due, then move.
pub fn tick(world: &mut World, dt: f64, alloc: Alloc<'_>) {
    let _span = soe_trace::Span::new("ai.tick", "game");
    let now = world.clock.real_seconds;
    for actor in world.loaded_actors() {
        if world.is_player(actor) {
            continue;
        }
        if world.actor_is_paralyzed(actor) {
            continue;
        }
        let Some((dead, unconscious)) = world.actor(actor).map(|a| (a.dead, a.unconscious)) else {
            continue;
        };
        if dead || unconscious {
            continue;
        }
        let ignore_combat = ignores_combat(world, actor);
        if ignore_combat {
            clear_combat_engagement(world, actor);
        }
        if world.is_calmed(actor) && world.actor_value(actor, "aggression") < 1.0 {
            clear_combat_engagement(world, actor);
        }
        let turn_undead_threat = world.turn_undead_threat(actor);
        if world.is_fleeing(actor) || turn_undead_threat.is_some() {
            if let Some(state) = world.actor(actor) {
                state.in_combat = false;
                state.combat_target = None;
                state.ai.reevaluate = true;
                state.ai.path.clear();
                state.ai.path_cells.clear();
                state.ai.destination = None;
                state.ai.arrived = false;
            }
            if let Some(threat) = turn_undead_threat {
                flee_turn_undead(world, actor, threat, dt as f32);
            } else if let Some(source) = world.flee_source(actor) {
                flee_from(world, actor, source, dt as f32);
            }
            continue;
        }
        let active_target = combat_target(world, actor);
        if active_target.is_some_and(|target| !target_in_guard_area(world, actor, target)) {
            clear_combat_engagement(world, actor);
        }
        let fleeing = world
            .actor(actor)
            .is_some_and(|state| state.ai.procedure == Procedure::Flee);
        if active_target.is_none() {
            let due = world.actor(actor).is_some_and(|state| {
                state.ai.package.is_none()
                    || state.ai.reevaluate
                    || now - state.ai.last_evaluation >= EVALUATION_INTERVAL
            });
            if due {
                evaluate(world, actor, alloc);
            }
        }
        if active_target.is_some() && !ignores_combat(world, actor) && !fleeing {
            let due = world.actor(actor).is_some_and(|a| {
                a.ai.package.is_none()
                    || a.ai.reevaluate
                    || now - a.ai.last_evaluation >= EVALUATION_INTERVAL
            });
            if due {
                evaluate(world, actor, alloc);
            }
            if !ignores_combat(world, actor)
                && world
                    .actor(actor)
                    .is_none_or(|state| state.ai.procedure != Procedure::Flee)
                && combat_target(world, actor).is_some()
            {
                pursue_combat_target(world, actor, dt as f32);
                continue;
            }
        } else if !ignore_combat
            && !ignores_combat(world, actor)
            && !fleeing
            && acquire_combat_target(world, actor)
        {
            pursue_combat_target(world, actor, dt as f32);
            continue;
        }
        let due = world.actor(actor).is_some_and(|a| {
            a.ai.package.is_none()
                || a.ai.reevaluate
                || now - a.ai.last_evaluation >= EVALUATION_INTERVAL
        });
        if due {
            evaluate(world, actor, &mut *alloc);
        }
        step(world, actor, dt as f32, &mut *alloc);
    }
}

/// Move away from a live hostile effect source along the vanilla navmesh.
fn flee_from(world: &mut World, actor: GlobalFormId, source: GlobalFormId, dt: f32) {
    let Some((position, cell)) = world
        .reference(actor)
        .map(|reference| (reference.position, reference.cell))
    else {
        return;
    };
    let Some(threat) = world.reference(source).map(|reference| reference.position) else {
        return;
    };
    let offset = [
        position[0] - threat[0],
        position[1] - threat[1],
        position[2] - threat[2],
    ];
    let length = distance(position, threat);
    if length <= f32::EPSILON {
        return;
    }
    let destination = std::array::from_fn(|axis| position[axis] + offset[axis] / length * 512.0);
    let Some((path, cells)) = plan_combat(world, position, destination, cell) else {
        return;
    };
    let speed = movement_speed(world, actor, speed::RUN);
    let mut budget = speed * dt.max(0.0);
    let mut position = position;
    let mut cell = cell;
    for (index, next) in path.iter().enumerate() {
        let d = distance(position, *next);
        if d <= budget {
            position = *next;
            budget -= d;
            if let Some(Some(next_cell)) = cells.get(index) {
                cell = Some(*next_cell);
            }
        } else if d > 0.0 {
            let fraction = budget / d;
            position = std::array::from_fn(|axis| {
                position[axis] + (next[axis] - position[axis]) * fraction
            });
            break;
        }
        if budget <= 0.0 {
            break;
        }
    }
    if let Some(reference) = world.reference(actor) {
        reference.position = position;
        if cell.is_some() {
            reference.cell = cell;
        }
    }
}

/// Run away from the caster while a Turn Undead application is active.
fn flee_turn_undead(world: &mut World, actor: GlobalFormId, threat: GlobalFormId, dt: f32) {
    let Some((mut position, mut cell)) = world
        .reference(actor)
        .map(|reference| (reference.position, reference.cell))
    else {
        return;
    };
    let Some(threat_position) = world.reference(threat).map(|reference| reference.position) else {
        return;
    };
    let mut direction = [
        position[0] - threat_position[0],
        position[1] - threat_position[1],
        position[2] - threat_position[2],
    ];
    let length = distance(position, threat_position);
    if length <= f32::EPSILON {
        direction = [1.0, 0.0, 0.0];
    } else {
        for component in &mut direction {
            *component /= length;
        }
    }
    let destination = std::array::from_fn(|axis| position[axis] + direction[axis] * 512.0);
    let (path, cells) = plan(world, position, destination, cell);
    let speed = movement_speed(world, actor, speed::RUN);
    let mut budget = speed * dt.max(0.0);
    for (index, next) in path.iter().enumerate() {
        if budget <= 0.0 {
            break;
        }
        let distance = distance(position, *next);
        if distance <= budget {
            position = *next;
            budget -= distance;
            if let Some(Some(next_cell)) = cells.get(index) {
                cell = Some(*next_cell);
            }
        } else if distance > 0.0 {
            let fraction = budget / distance;
            position = std::array::from_fn(|axis| {
                position[axis] + (next[axis] - position[axis]) * fraction
            });
            budget = 0.0;
        }
    }
    if let Some(reference) = world.reference(actor) {
        reference.position = position;
        if cell.is_some() {
            reference.cell = cell;
        }
    }
}

fn ignores_combat(world: &mut World, actor: GlobalFormId) -> bool {
    world
        .actor(actor)
        .and_then(|state| state.ai.package)
        .and_then(|id| package(world, id))
        .is_some_and(|pack| pack.has_flag(flags::IGNORE_COMBAT))
}

fn clear_combat_engagement(world: &mut World, actor: GlobalFormId) {
    if let Some(state) = world.actor(actor) {
        if !state.in_combat && state.combat_target.is_none() {
            return;
        }
        state.in_combat = false;
        state.combat_target = None;
        state.ai.reevaluate = true;
        state.ai.path.clear();
        state.ai.path_cells.clear();
        state.ai.destination = None;
        state.ai.arrived = false;
    }
}

/// Drop dead or missing targets, otherwise return the current target.
fn combat_target(world: &mut World, actor: GlobalFormId) -> Option<GlobalFormId> {
    let target = world
        .actor(actor)
        .filter(|state| state.in_combat)
        .and_then(|state| state.combat_target);
    let target = target?;
    let valid = world.actor(target).is_some_and(|state| !state.dead);
    if valid {
        return Some(target);
    }
    clear_combat_engagement(world, actor);
    if let Some(state) = world.actor(actor) {
        state.ai.reevaluate = true;
    }
    None
}

/// A Guard only engages targets inside its assigned area.
fn target_in_guard_area(world: &mut World, actor: GlobalFormId, target: GlobalFormId) -> bool {
    let Some(state) = world.actor(actor) else {
        return false;
    };
    if state.ai.procedure != Procedure::Guard {
        return true;
    }
    let Some(center) = state.ai.center else {
        return true;
    };
    let radius = state.ai.radius.max(ARRIVAL_DISTANCE);
    world
        .reference(target)
        .is_some_and(|reference| distance(center, reference.position) <= radius)
}

/// Acquire the nearest detected target for an aggressive actor.
fn acquire_combat_target(world: &mut World, actor: GlobalFormId) -> bool {
    if world
        .actor(actor)
        .is_none_or(|state| state.dead || state.unconscious)
    {
        return false;
    }
    let aggression = world.actor_value(actor, "aggression");
    if aggression < 1.0 {
        return false;
    }
    if ignores_combat(world, actor) {
        return false;
    }
    let Some(from) = world.reference(actor).map(|reference| reference.position) else {
        return false;
    };
    let frenzied = world.is_frenzied(actor);
    let mut candidates = world.loaded_actors();
    candidates.sort();
    let mut best: Option<(GlobalFormId, f32)> = None;
    for target in candidates {
        if target == actor
            || world
                .actor(target)
                .is_none_or(|state| state.dead || state.unconscious || state.teammate && !frenzied)
            || !world.awareness(actor, target).is_some_and(|awareness| {
                awareness.level == crate::detection::DetectionLevel::Combat
            })
        {
            continue;
        }
        let Some(position) = world.reference(target).map(|reference| reference.position) else {
            continue;
        };
        let in_guard_area = world.actor(actor).is_some_and(|state| {
            state.ai.procedure != Procedure::Guard
                || state.ai.center.is_none_or(|center| {
                    distance(center, position) <= state.ai.radius.max(ARRIVAL_DISTANCE)
                })
        });
        if !in_guard_area {
            continue;
        }
        let candidate = (target, distance(from, position));
        if best.is_none_or(|current| {
            candidate
                .1
                .total_cmp(&current.1)
                .then_with(|| candidate.0.cmp(&current.0))
                .is_lt()
        }) {
            best = Some(candidate);
        }
    }
    let target = best.map(|(target, _)| target);
    let Some(target) = target else {
        return false;
    };
    let no_combat_alert = world
        .actor(actor)
        .and_then(|state| state.ai.package)
        .and_then(|id| package(world, id))
        .is_some_and(|package| package.has_flag(flags::NO_COMBAT_ALERT));
    if let Some(state) = world.actor(actor) {
        state.in_combat = true;
        state.combat_target = Some(target);
        if !no_combat_alert {
            state.alerted = true;
        }
        state.ai.reevaluate = true;
        state.ai.path.clear();
        state.ai.path_cells.clear();
        state.ai.destination = None;
        state.ai.arrived = false;
    }
    true
}

/// Advance an engaged actor along the shared vanilla NAVM graph.
fn pursue_combat_target(world: &mut World, actor: GlobalFormId, dt: f32) {
    let Some(target) = combat_target(world, actor) else {
        return;
    };
    let Some((mut position, mut cell)) = world
        .reference(actor)
        .map(|reference| (reference.position, reference.cell))
    else {
        return;
    };
    let Some((destination, target_cell)) = world
        .reference(target)
        .map(|reference| (reference.position, reference.cell))
    else {
        return;
    };
    let Some(speed) = world
        .actor(actor)
        .map(|state| state.ai.speed)
        .filter(|speed| *speed > 0.0)
        .or_else(|| Some(movement_speed(world, actor, speed::RUN)))
    else {
        return;
    };
    let remaining = distance(position, destination);
    let Some(state) = world.actor(actor) else {
        return;
    };
    if remaining <= COMBAT_ARRIVAL_DISTANCE {
        state.ai.destination = Some(destination);
        state.ai.path.clear();
        state.ai.path_cells.clear();
        state.ai.arrived = true;
        return;
    }
    let mut ai = std::mem::take(&mut state.ai);
    if ai
        .destination
        .is_none_or(|old| distance(old, destination) > COMBAT_ARRIVAL_DISTANCE)
        || ai.path.is_empty()
    {
        let Some((path, cells)) = plan_combat(world, position, destination, target_cell) else {
            if let Some(state) = world.actor(actor) {
                state.ai = ai;
            }
            return;
        };
        ai.destination = Some(destination);
        ai.path = path;
        ai.path_cells = cells;
    }
    let mut budget = speed * dt.max(0.0);
    while budget > 0.0 && !ai.path.is_empty() {
        let next = ai.path[0];
        let d = distance(position, next);
        if d <= budget {
            position = next;
            budget -= d;
            ai.path.remove(0);
            if let Some(Some(next_cell)) = ai.path_cells.first() {
                cell = Some(*next_cell);
            }
            if !ai.path_cells.is_empty() {
                ai.path_cells.remove(0);
            }
        } else if d > 0.0 {
            let fraction = budget / d;
            position = std::array::from_fn(|axis| {
                position[axis] + (next[axis] - position[axis]) * fraction
            });
            budget = 0.0;
        } else {
            ai.path.remove(0);
            if !ai.path_cells.is_empty() {
                ai.path_cells.remove(0);
            }
        }
    }
    ai.arrived = distance(position, destination) <= COMBAT_ARRIVAL_DISTANCE;
    if let Some(reference) = world.reference(actor) {
        reference.position = position;
        if cell.is_some() {
            reference.cell = cell;
        }
    }
    if let Some(state) = world.actor(actor) {
        state.ai = ai;
    }
}

/// Plan combat movement only when the vanilla navmesh contains a route.
fn plan_combat(
    world: &mut World,
    from: [f32; 3],
    to: [f32; 3],
    to_cell: Option<GlobalFormId>,
) -> Option<PlannedPath> {
    ensure_nav_graph(world);
    let path = world.ai.nav.as_ref()?.find_path_between(from, to)?;
    let cells = path
        .point_nodes
        .iter()
        .skip(1)
        .map(|node| {
            world
                .ai
                .nav_cells
                .get(node.mesh as usize)
                .copied()
                .flatten()
                .or(to_cell)
        })
        .collect();
    Some((path.points[1..].to_vec(), cells))
}

/// Choose a reachable navmesh point that increases distance from the threat.
fn plan_flee(
    world: &mut World,
    from: [f32; 3],
    threat: [f32; 3],
) -> Option<([f32; 3], PlannedPath)> {
    ensure_nav_graph(world);
    let graph = world.ai.nav.as_ref()?;
    let start = graph.locate(from).or_else(|| graph.nearest(from))?;
    let start_distance = distance(from, threat);
    let mut candidates: Vec<_> = graph
        .reachable(start)
        .into_iter()
        .filter(|node| !graph.triangle(*node).blocked)
        .map(|node| (node, graph.centroid(node)))
        .filter(|(_, point)| distance(*point, threat) > start_distance + f32::EPSILON)
        .collect();
    candidates.sort_by(|(a_node, a), (b_node, b)| {
        distance(*b, threat)
            .total_cmp(&distance(*a, threat))
            .then_with(|| a_node.cmp(b_node))
    });
    for (_node, destination) in candidates {
        let Some(path) = graph.find_path_between(from, destination) else {
            continue;
        };
        let cells = path
            .point_nodes
            .iter()
            .skip(1)
            .map(|point| {
                world
                    .ai
                    .nav_cells
                    .get(point.mesh as usize)
                    .copied()
                    .flatten()
            })
            .collect();
        return Some((destination, (path.points[1..].to_vec(), cells)));
    }
    None
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Plan the waypoints from `from` to `to` over the graph, with the cell the
/// actor enters at each of them. Off the graph the route is a straight
/// line that ends in `to_cell`.
fn plan(
    world: &mut World,
    from: [f32; 3],
    to: [f32; 3],
    to_cell: Option<GlobalFormId>,
) -> PlannedPath {
    ensure_nav_graph(world);
    let (Some(graph), nav_cells) = (world.ai.nav.as_ref(), &world.ai.nav_cells) else {
        return (vec![to], vec![to_cell]);
    };
    if let Some(path) = graph.find_path_between(from, to) {
        let cells: Vec<Option<GlobalFormId>> = path
            .point_nodes
            .iter()
            .skip(1)
            .map(|node| nav_cells.get(node.mesh as usize).copied().flatten())
            .collect();
        return (path.points[1..].to_vec(), cells);
    }
    (vec![to], vec![to_cell])
}

fn step(world: &mut World, actor: GlobalFormId, dt: f32, alloc: Alloc<'_>) {
    let Some((position, cell)) = world.reference(actor).map(|r| (r.position, r.cell)) else {
        return;
    };
    let Some(mut ai) = world.actor(actor).map(|a| std::mem::take(&mut a.ai)) else {
        return;
    };
    if ai.tree.is_none() && ai.package.is_some() {
        tree::start(world, actor, &mut ai, &mut *alloc);
    } else {
        tree::enter(world, actor, &mut ai, &mut *alloc);
    }
    if ai.procedure == Procedure::UseWeapon && ai.tree.as_ref().is_some_and(|tree| tree.active) {
        let target = world.actor(actor).and_then(|state| state.combat_target);
        let weapon = world
            .hand_weapon(actor, crate::combat::Hand::Right)
            .or_else(|| world.hand_weapon(actor, crate::combat::Hand::Left));
        if let (Some(target), Some(weapon)) = (target, weapon) {
            let _ = world.melee_hit(
                crate::combat::MeleeHit {
                    attacker: actor,
                    target,
                    weapon,
                    is_bash: false,
                    is_power_attack: false,
                },
                Vec::new(),
                Vec::new(),
                alloc,
            );
        }
        tree::succeeded(world, &mut ai);
    }
    if ai.procedure == Procedure::Patrol && ai.tree.as_ref().is_some_and(|tree| tree.active) {
        patrol::prepare(world, &mut ai);
    }
    if ai.procedure == Procedure::Flee && ai.tree.as_ref().is_some_and(|tree| tree.active) {
        let target = combat_target(world, actor);
        if let Some(threat) = target.and_then(|target| world.reference(target).map(|r| r.position))
        {
            let from = world
                .reference(actor)
                .map(|r| r.position)
                .unwrap_or(position);
            let needs_route = ai.destination.is_none()
                || ai.path.is_empty()
                || ai.destination.is_some_and(|destination| {
                    distance(destination, threat) <= distance(from, threat) + f32::EPSILON
                });
            if needs_route {
                if let Some((destination, (path, cells))) = plan_flee(world, from, threat) {
                    ai.destination = Some(destination);
                    ai.path = path;
                    ai.path_cells = cells;
                    ai.arrived = false;
                } else {
                    ai.destination = None;
                    ai.path.clear();
                    ai.path_cells.clear();
                    ai.arrived = false;
                }
            }
        } else {
            tree::succeeded(world, &mut ai);
            ai.destination = None;
            ai.path.clear();
            ai.path_cells.clear();
            ai.arrived = false;
        }
    }
    if ai.procedure == Procedure::Follow {
        follow_target(world, actor, &mut ai, position);
    }
    if matches!(
        ai.procedure,
        Procedure::ForceGreet | Procedure::Activate | Procedure::DialogueActivate
    ) {
        approach_assigned_target(world, actor, &mut ai, position);
    }
    if matches!(ai.procedure, Procedure::Orbit { .. }) {
        orbit(world, actor, &mut ai, position, dt);
        if let Some(a) = world.actor(actor) {
            a.ai = ai;
        }
        return;
    }
    if ai.procedure == Procedure::HoldPosition {
        let hold_position = ai.center.unwrap_or(position);
        if distance(position, hold_position) > ARRIVAL_DISTANCE {
            ai.destination = Some(hold_position);
            ai.path.clear();
            ai.path_cells.clear();
            ai.arrived = false;
        } else {
            ai.destination = None;
            ai.path.clear();
            ai.path_cells.clear();
            ai.arrived = true;
        }
    }
    if matches!(ai.procedure, Procedure::Wait { .. })
        && ai
            .wait_until
            .is_some_and(|until| world.clock.real_seconds >= until)
    {
        tree::succeeded(world, &mut ai);
    }
    if ai.procedure == Procedure::Escort && !ai.arrived {
        if escort_can_advance(world, actor, &ai, position) {
            if ai.destination.is_none() {
                ai.destination = ai.center;
                ai.path.clear();
                ai.path_cells.clear();
            }
        } else {
            ai.destination = None;
            ai.path.clear();
            ai.path_cells.clear();
        }
    }
    if ai.procedure == Procedure::Shout {
        if let Some(until) = ai.wait_until {
            if world.clock.real_seconds >= until {
                tree::succeeded(world, &mut ai);
            }
        } else if world.voice_recovery_time(actor) <= 0.0 {
            let target = world.actor(actor).and_then(|state| state.combat_target);
            if let Err(error) = world.cast_shout(actor, target, alloc) {
                ai.procedure = Procedure::Unsupported(format!("Shout: {error}"));
            } else {
                let recovery = world.voice_recovery_time(actor);
                if recovery > 0.0 {
                    ai.wait_until = Some(world.clock.real_seconds + f64::from(recovery));
                } else {
                    tree::succeeded(world, &mut ai);
                }
            }
        }
    }
    let hours = world.clock.game_hours();
    if ai.arrived {
        match ai.procedure {
            Procedure::Sandbox if hours >= ai.dwell_until => {
                ai.arrived = false;
                ai.destination = None;
            }
            Procedure::Sleep => ai.sleeping = true,
            _ => {}
        }
    }
    if ai.procedure == Procedure::Guard {
        if let Some(center) = ai.center {
            if distance(position, center) <= ai.radius.max(ARRIVAL_DISTANCE) {
                ai.arrived = true;
                ai.destination = None;
                ai.path.clear();
                ai.path_cells.clear();
            } else if ai.destination.is_none() || ai.arrived {
                ai.arrived = false;
                ai.destination = Some(center);
                ai.path.clear();
                ai.path_cells.clear();
            }
        }
    }
    if !ai.arrived && ai.destination.is_none() && ai.procedure == Procedure::Sandbox {
        let center = ai.center.unwrap_or(position);
        let markers = ai
            .center_cell
            .or(cell)
            .map(|c| sandbox_markers(world, c, center, ai.radius))
            .unwrap_or_default();
        let target = if markers.is_empty() {
            let angle = world.random_float(0.0, std::f32::consts::TAU);
            let r = world.random_float(0.0, ai.radius);
            [
                center[0] + r * angle.cos(),
                center[1] + r * angle.sin(),
                center[2],
            ]
        } else {
            let pick = world.random_int(0, markers.len() as i32 - 1) as usize;
            world
                .reference(markers[pick])
                .map(|s| s.position)
                .unwrap_or(center)
        };
        ai.destination = Some(target);
        ai.path.clear();
        ai.path_cells.clear();
    }
    if let Procedure::UseIdleMarker { marker: None } = ai.procedure {
        let center = ai.center.unwrap_or(position);
        let markers = ai
            .center_cell
            .or(cell)
            .map(|c| idle_markers(world, actor, c, center, ai.radius))
            .unwrap_or_default();
        if !markers.is_empty() {
            let pick = world.random_int(0, markers.len() as i32 - 1) as usize;
            let marker = markers[pick];
            if let Some(target) = world.reference(marker).map(|reference| reference.position) {
                ai.procedure = Procedure::UseIdleMarker {
                    marker: Some(marker),
                };
                ai.destination = Some(target);
                ai.path.clear();
                ai.path_cells.clear();
            }
        }
    }
    if ai.procedure == Procedure::Patrol
        && !ai.arrived
        && ai
            .destination
            .is_some_and(|dest| distance(position, dest) <= ai.radius)
    {
        ai.arrived = true;
        ai.path.clear();
        ai.path_cells.clear();
        if let (Some(target_cell), Some(reference)) = (ai.center_cell, world.reference(actor)) {
            reference.cell = Some(target_cell);
        }
    }
    if let (false, Some(dest)) = (ai.arrived, ai.destination) {
        if ai.path.is_empty() {
            let (path, cells) = plan(world, position, dest, ai.center_cell);
            ai.path = path;
            ai.path_cells = cells;
        }
        let mut pos = position;
        let mut new_cell = cell;
        let mut budget = ai.speed * dt;
        while budget > 0.0 && !ai.path.is_empty() {
            let next = ai.path[0];
            let d = distance(pos, next);
            if ai.procedure == Procedure::Patrol
                && ai.path.len() == 1
                && ai.radius > 0.0
                && d - ai.radius <= budget
            {
                let t = ((d - ai.radius) / d).clamp(0.0, 1.0);
                pos = std::array::from_fn(|axis| pos[axis] + (next[axis] - pos[axis]) * t);
                if let Some(Some(target_cell)) = ai.path_cells.first() {
                    new_cell = Some(*target_cell);
                }
                ai.path.clear();
                ai.path_cells.clear();
                ai.arrived = true;
                break;
            }
            let waypoint_budget = if ai.procedure == Procedure::Patrol {
                budget
            } else {
                budget.max(ARRIVAL_DISTANCE)
            };
            if d <= waypoint_budget {
                pos = next;
                budget -= d;
                ai.path.remove(0);
                if !ai.path_cells.is_empty() {
                    if let Some(c) = ai.path_cells.remove(0) {
                        new_cell = Some(c);
                    }
                }
            } else {
                let t = budget / d;
                pos = [
                    pos[0] + (next[0] - pos[0]) * t,
                    pos[1] + (next[1] - pos[1]) * t,
                    pos[2] + (next[2] - pos[2]) * t,
                ];
                budget = 0.0;
            }
        }
        if ai.path.is_empty() && distance(pos, dest) <= ARRIVAL_DISTANCE {
            ai.arrived = true;
            if ai.procedure == Procedure::Sandbox {
                ai.dwell_until = hours + f64::from(world.random_float(0.1, 0.4));
            }
            if matches!(ai.procedure, Procedure::Travel | Procedure::Flee) {
                tree::succeeded(world, &mut ai);
            } else if ai.procedure == Procedure::Sit {
                world.stand_from_furniture(actor);
                tree::succeeded(world, &mut ai);
            }
            if ai.procedure == Procedure::ForceGreet {
                ai.arrived = true;
            }
        }
        if let Some(r) = world.reference(actor) {
            r.position = pos;
            if new_cell.is_some() {
                r.cell = new_cell;
            }
        }
    }
    if ai.procedure == Procedure::Patrol {
        patrol::tick(world, actor, &mut ai, alloc);
    }
    if ai.arrived && ai.procedure == Procedure::Escort && escort_target_arrived(world, actor, &ai) {
        tree::succeeded(world, &mut ai);
    }
    if let Some(a) = world.actor(actor) {
        a.ai = ai;
    }
}

fn follow_target(world: &mut World, actor: GlobalFormId, ai: &mut AiState, position: [f32; 3]) {
    let Some((owner, target_data, catch_up_radius, follow_radius)) =
        tree::follow_parameters(world, ai)
    else {
        ai.procedure = Procedure::Unsupported("Follow inputs".into());
        return;
    };
    let Some(target) = resolve_follow_target(world, actor, ai, (owner, target_data)) else {
        ai.procedure = Procedure::Unsupported("Follow target".into());
        return;
    };
    let Some((target_position, target_cell)) = world
        .reference(target)
        .map(|reference| (reference.position, reference.cell))
    else {
        ai.procedure = Procedure::Unsupported("Follow target is not placed".into());
        return;
    };
    let separation = distance(position, target_position);
    if separation <= follow_radius || separation == 0.0 {
        ai.destination = None;
        ai.path.clear();
        ai.path_cells.clear();
        ai.arrived = true;
        return;
    }
    let desired_distance = follow_radius.min(separation);
    let toward_target: [f32; 3] =
        std::array::from_fn(|axis| (target_position[axis] - position[axis]) / separation);
    let destination: [f32; 3] =
        std::array::from_fn(|axis| target_position[axis] - toward_target[axis] * desired_distance);
    ai.destination = Some(destination);
    ai.center_cell = target_cell;
    ai.path.clear();
    ai.path_cells.clear();
    ai.arrived = false;
    let preferred = if separation > catch_up_radius {
        speed::RUN
    } else {
        ai.package
            .and_then(|id| package(world, id))
            .filter(|p| p.has_flag(flags::PREFERRED_SPEED))
            .map_or(speed::WALK, |p| p.general.preferred_speed)
    };
    ai.speed = movement_speed(world, actor, preferred);
}

fn approach_assigned_target(
    world: &mut World,
    actor: GlobalFormId,
    ai: &mut AiState,
    position: [f32; 3],
) {
    if ai.force_greet_started || ai.activate_started || ai.dialogue_activate_started {
        return;
    }
    let target_data = match ai.procedure {
        Procedure::ForceGreet => tree::force_greet_parameters(world, ai).map(|(target, _)| target),
        Procedure::Activate | Procedure::DialogueActivate => tree::activate_target(world, ai),
        _ => None,
    };
    let target = target_data
        .and_then(|data| resolve_follow_target(world, actor, ai, data))
        .and_then(|id| {
            world
                .reference(id)
                .map(|reference| (reference.position, reference.cell))
        });
    let Some((target_position, target_cell)) = target else {
        ai.procedure = Procedure::Unsupported("assigned target is not placed".into());
        ai.destination = None;
        ai.path.clear();
        ai.path_cells.clear();
        return;
    };
    if target_position.iter().any(|value| !value.is_finite()) {
        ai.procedure = Procedure::Unsupported("assigned target position".into());
        ai.destination = None;
        ai.path.clear();
        ai.path_cells.clear();
        return;
    }
    if distance(position, target_position) <= ARRIVAL_DISTANCE {
        ai.destination = None;
        ai.path.clear();
        ai.path_cells.clear();
        ai.arrived = true;
    } else if ai.destination != Some(target_position) || ai.center_cell != target_cell || ai.arrived
    {
        ai.destination = Some(target_position);
        ai.center_cell = target_cell;
        ai.path.clear();
        ai.path_cells.clear();
        ai.arrived = false;
    }
}

fn orbit(world: &mut World, actor: GlobalFormId, ai: &mut AiState, position: [f32; 3], dt: f32) {
    let Some((owner, location, inner, outer, height)) = tree::orbit_parameters(world, ai) else {
        ai.procedure = Procedure::Unsupported("Orbit inputs, AI-25".into());
        return;
    };
    if let Some(place) = resolve_location(world, actor, owner, ai.alias, ai, &location) {
        ai.center = Some(place.position);
        ai.center_cell = place.cell;
    } else {
        ai.procedure = Procedure::Unsupported("Orbit target location, AI-25".into());
        return;
    }
    let Some(center) = ai.center else { return };
    let center = [center[0], center[1], center[2] + height];
    let dx = position[0] - center[0];
    let dy = position[1] - center[1];
    let current_radius = dx.hypot(dy);
    let radius = current_radius.clamp(inner, outer);
    let angle = dy.atan2(dx);
    let angular_step = (ai.speed * dt.max(0.0) / radius.max(1.0)).clamp(0.0, 0.3);
    let mut moved = false;
    for direction in [1.0, -1.0] {
        let next_angle = angle + direction * angular_step;
        let destination = [
            center[0] + radius * next_angle.cos(),
            center[1] + radius * next_angle.sin(),
            center[2],
        ];
        ensure_nav_graph(world);
        let Some(graph) = world.ai.nav.as_ref() else {
            return;
        };
        if graph.find_path_between(position, destination).is_none()
            || !orbit_arc_valid(graph, center, radius, angle, next_angle, inner, outer)
        {
            continue;
        }
        if let Some(reference) = world.reference(actor) {
            reference.position = destination;
            if let Some(cell) = ai.center_cell {
                reference.cell = Some(cell);
            }
        }
        ai.destination = Some(destination);
        ai.arrived = true;
        moved = true;
        break;
    }
    if !moved {
        ai.destination = None;
        ai.arrived = false;
    }
}

fn orbit_arc_valid(
    graph: &NavGraph,
    center: [f32; 3],
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    inner: f32,
    outer: f32,
) -> bool {
    (0..=8).all(|step| {
        let t = step as f32 / 8.0;
        let angle = start_angle + (end_angle - start_angle) * t;
        let point = [
            center[0] + radius * angle.cos(),
            center[1] + radius * angle.sin(),
            center[2],
        ];
        let radius = (point[0] - center[0]).hypot(point[1] - center[1]);
        (inner - 0.01..=outer + 0.01).contains(&radius) && graph.locate(point).is_some()
    })
}

// ----- save and load -----

fn write_opt_form_id(w: &mut Writer, id: Option<GlobalFormId>) {
    match id {
        Some(id) => {
            w.bool(true);
            w.u16(id.file);
            w.u32(id.object);
        }
        None => w.bool(false),
    }
}

fn write_point(w: &mut Writer, p: [f32; 3]) {
    for v in p {
        w.f32(v);
    }
}

fn read_point(r: &mut Reader<'_>) -> Result<[f32; 3], VmError> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}

/// Write one actor's driver record. The procedure cursor is not part of it,
/// the world save keeps cursors in their own block.
pub(crate) fn write_record_state(w: &mut Writer, a: &AiState) {
    write_record_state_version(w, a, 43);
}

/// Write the actor record layout that predates the named AI save section.
pub(crate) fn write_legacy_record_state(w: &mut Writer, a: &AiState) {
    write_record_state_version(w, a, 33);
}

fn write_record_state_version(w: &mut Writer, a: &AiState, version: u16) {
    write_opt_form_id(w, a.package);
    match a.alias {
        Some((q, id)) => {
            w.bool(true);
            w.u16(q.file);
            w.u32(q.object);
            w.u32(id);
        }
        None => w.bool(false),
    }
    match &a.procedure {
        Procedure::None | Procedure::UseMagic | Procedure::UseWeapon => w.u8(0),
        Procedure::Travel => w.u8(1),
        Procedure::Sandbox => w.u8(2),
        Procedure::Sleep => w.u8(3),
        Procedure::Flee => w.u8(if version >= 37 { 19 } else { 7 }),
        Procedure::Patrol => {
            w.u8(6);
            patrol::write(w, a.patrol.as_ref());
        }
        Procedure::Guard => {
            // Reuse the named-procedure representation without changing the
            // actor record layout or its save version.
            w.u8(4);
            w.str("AI-30 Guard");
        }
        Procedure::Sit => w.u8(if version >= 37 { 18 } else { 9 }),
        Procedure::Wait { stop_movement } => {
            w.u8(5);
            w.bool(*stop_movement);
            w.bool(a.wait_until.is_some());
            if let Some(until) = a.wait_until {
                w.f64(until);
            }
        }
        Procedure::Hover if version >= 34 => {
            w.u8(7);
            w.f32(a.hover_height);
        }
        Procedure::Follow if version >= 35 => w.u8(8),
        Procedure::ForceGreet if version >= 37 => w.u8(10),
        Procedure::DialogueActivate if version >= 43 => w.u8(16),
        Procedure::HoldPosition if version >= 38 => w.u8(11),
        Procedure::Orbit { height } if version >= 39 => {
            w.u8(12);
            w.f32(*height);
        }
        Procedure::Escort if version >= 40 => w.u8(13),
        Procedure::Activate if version >= 41 => w.u8(14),
        Procedure::Find if version >= 42 => w.u8(15),
        Procedure::UseIdleMarker { marker } if version >= 36 => {
            w.u8(9);
            match marker {
                Some(marker) => {
                    w.bool(true);
                    w.u16(marker.file);
                    w.u32(marker.object);
                }
                None => w.bool(false),
            }
        }
        Procedure::Shout => {
            w.u8(17);
            w.bool(a.wait_until.is_some());
            if let Some(until) = a.wait_until {
                w.f64(until);
            }
        }
        Procedure::Unsupported(name) => {
            w.u8(4);
            w.str(name);
        }
        _ => {
            w.u8(4);
            w.str("procedure stored in actor section");
        }
    }
    match a.destination {
        Some(p) => {
            w.bool(true);
            write_point(w, p);
        }
        None => w.bool(false),
    }
    w.len(a.path.len());
    for p in &a.path {
        write_point(w, *p);
    }
    w.len(a.path_cells.len());
    for c in &a.path_cells {
        write_opt_form_id(w, *c);
    }
    w.bool(a.arrived);
    if version >= 37 {
        w.bool(a.force_greet_started);
    }
    if version >= 41 {
        w.bool(a.activate_started);
    }
    w.bool(a.sleeping);
    write_opt_form_id(w, a.bed);
    match a.center {
        Some(p) => {
            w.bool(true);
            write_point(w, p);
        }
        None => w.bool(false),
    }
    write_opt_form_id(w, a.center_cell);
    w.f32(a.radius);
    w.f64(a.dwell_until);
    w.f32(a.speed);
    write_point(w, a.start_position);
    w.bool(a.reevaluate);
    w.f64(a.last_evaluation);
    w.len(a.completed.len());
    for (id, day) in &a.completed {
        w.u16(id.file);
        w.u32(id.object);
        w.u32(*day);
    }
    if version >= 42 {
        write_opt_form_id(w, a.found_reference);
    }
    if version >= 43 {
        w.bool(a.dialogue_activate_started);
    }
}

/// Write one actor's package procedure cursor. The world save keeps these in
/// their own block so an actor record stays at its established layout.
pub(crate) fn write_tree_state(w: &mut Writer, tree: Option<&TreeState>) {
    w.bool(tree.is_some());
    if let Some(tree) = tree {
        w.u16(tree.owner.file);
        w.u32(tree.owner.object);
        w.u32(tree.branch);
        w.bool(tree.active);
        w.bool(tree.finished);
    }
}

/// Write a complete driver state, record and procedure cursor together.
pub fn write_state(w: &mut Writer, a: &AiState) {
    write_record_state(w, a);
    write_tree_state(w, a.tree.as_ref());
}

/// Read a complete driver state written by [`write_state`].
pub fn read_state(r: &mut Reader<'_>) -> Result<AiState, VmError> {
    let mut a = read_state_mapped(
        &mut FormReader::new(r, FormNamespace::Current),
        true,
        true,
        true,
        43,
    )?;
    a.tree = read_tree_state(r)?;
    Ok(a)
}

pub(crate) fn read_state_mapped(
    r: &mut FormReader<'_, '_>,
    has_force_greet_state: bool,
    has_hold_position: bool,
    has_orbit: bool,
    world_version: u16,
) -> Result<AiState, VmError> {
    let mut a = AiState {
        package: read_opt_form_id(r)?,
        ..Default::default()
    };
    if r.bool()? {
        let q = read_form_id(r)?;
        a.alias = Some((q, r.u32()?));
    }
    a.procedure = match r.u8()? {
        0 => Procedure::None,
        1 => Procedure::Travel,
        2 => Procedure::Sandbox,
        3 => Procedure::Sleep,
        7 if world_version < 34 => Procedure::Flee,
        7 => {
            let height = r.f32()?;
            if !height.is_finite() {
                return Err(VmError::State("invalid Hover height".into()));
            }
            a.hover_height = height;
            Procedure::Hover
        }
        19 => Procedure::Flee,
        4 => match r.str()?.as_str() {
            "AI-30 Guard" => Procedure::Guard,
            other => Procedure::Unsupported(other.to_owned()),
        },
        6 => {
            a.patrol = Some(patrol::read(r)?);
            Procedure::Patrol
        }
        9 if world_version < 37 => Procedure::Sit,
        5 => {
            let stop_movement = r.bool()?;
            if r.bool()? {
                let until = r.f64()?;
                if !until.is_finite() || until < 0.0 {
                    return Err(VmError::State("invalid Wait deadline".into()));
                }
                a.wait_until = Some(until);
            }
            Procedure::Wait { stop_movement }
        }
        8 => Procedure::Follow,
        10 => Procedure::ForceGreet,
        11 if has_hold_position => Procedure::HoldPosition,
        11 => {
            return Err(VmError::State(
                "HoldPosition procedure requires world version 38".into(),
            ));
        }
        12 if has_orbit => {
            let height = r.f32()?;
            if !height.is_finite() {
                return Err(VmError::State("invalid Orbit height".into()));
            }
            Procedure::Orbit { height }
        }
        12 => {
            return Err(VmError::State(
                "Orbit procedure requires world version 39".into(),
            ));
        }
        13 if world_version >= 40 => Procedure::Escort,
        13 => {
            return Err(VmError::State(
                "Escort procedure requires world version 40".into(),
            ));
        }
        14 if world_version >= 41 => Procedure::Activate,
        14 => {
            return Err(VmError::State(
                "Activate procedure requires world version 41".into(),
            ));
        }
        15 if world_version >= 42 => Procedure::Find,
        15 => {
            return Err(VmError::State(
                "Find procedure requires world version 42".into(),
            ));
        }
        16 if world_version >= 43 => Procedure::DialogueActivate,
        16 => {
            return Err(VmError::State(
                "DialogueActivate procedure requires world version 43".into(),
            ));
        }
        9 => {
            let marker = if r.bool()? {
                Some(read_form_id(r)?)
            } else {
                None
            };
            Procedure::UseIdleMarker { marker }
        }
        18 if world_version >= 37 => Procedure::Sit,
        18 => {
            return Err(VmError::State(
                "Sit procedure requires world version 37".into(),
            ))
        }
        17 => {
            if r.bool()? {
                let until = r.f64()?;
                if !until.is_finite() || until < 0.0 {
                    return Err(VmError::State("invalid Shout deadline".into()));
                }
                a.wait_until = Some(until);
            }
            Procedure::Shout
        }
        other => return Err(VmError::State(format!("unknown procedure {other}"))),
    };
    if r.bool()? {
        a.destination = Some(read_point(r)?);
    }
    let n = r.len()?;
    for _ in 0..n {
        a.path.push(read_point(r)?);
    }
    let n = r.len()?;
    for _ in 0..n {
        a.path_cells.push(read_opt_form_id(r)?);
    }
    a.arrived = r.bool()?;
    a.force_greet_started = has_force_greet_state && r.bool()?;
    a.activate_started = world_version >= 41 && r.bool()?;
    a.sleeping = r.bool()?;
    a.bed = read_opt_form_id(r)?;
    if r.bool()? {
        a.center = Some(read_point(r)?);
    }
    a.center_cell = read_opt_form_id(r)?;
    a.radius = r.f32()?;
    a.dwell_until = r.f64()?;
    a.speed = r.f32()?;
    a.start_position = read_point(r)?;
    a.reevaluate = r.bool()?;
    a.last_evaluation = r.f64()?;
    let n = r.len()?;
    for _ in 0..n {
        let id = read_form_id(r)?;
        a.completed.insert(id, r.u32()?);
    }
    if world_version >= 42 {
        a.found_reference = read_opt_form_id(r)?;
    }
    a.dialogue_activate_started = world_version >= 43 && r.bool()?;
    Ok(a)
}

pub(crate) fn read_tree_state(r: &mut Reader<'_>) -> Result<Option<TreeState>, VmError> {
    if !r.bool()? {
        return Ok(None);
    }
    let state = TreeState {
        owner: GlobalFormId {
            file: r.u16()?,
            object: r.u32()?,
        },
        branch: r.u32()?,
        active: r.bool()?,
        finished: r.bool()?,
    };
    if state.active && state.finished {
        return Err(VmError::State(
            "finished package tree has an active child".into(),
        ));
    }
    Ok(Some(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_cells_follow_portal_ownership_including_gap_departures() {
        for gap in [false, true] {
            let mut graph = NavGraph::new();
            for i in 0..3 {
                let x = (i * 100 + if gap && i == 2 { 100 } else { 0 }) as f32;
                graph.add_mesh_with_adjacency(
                    vec![
                        [x, 0.0, 0.0],
                        [x + 100.0, 0.0, 0.0],
                        [x + 100.0, 100.0, 0.0],
                        [x, 100.0, 0.0],
                    ],
                    &[[0, 1, 2], [0, 2, 3]],
                );
                if i > 0 {
                    graph.link(
                        soe_navmesh::NodeId::new(i - 1, 0),
                        1,
                        soe_navmesh::NodeId::new(i, 1),
                    );
                }
            }
            let cells: Vec<_> = (0..3)
                .map(|i| {
                    Some(GlobalFormId {
                        file: 0,
                        object: 100 + i,
                    })
                })
                .collect();
            let mut world = World::new(soe_esm::LoadOrder::new());
            world.ai.nav = Some(graph);
            world.ai.nav_cells = cells.clone();
            let end = [if gap { 390.0 } else { 290.0 }, 25.0, 0.0];
            let (points, owners) = plan(&mut world, [10.0, 25.0, 0.0], end, cells[2]);
            assert_eq!(points.len(), owners.len());
            let first = points
                .iter()
                .position(|p| (p[0] - 100.0).abs() < 0.01)
                .unwrap();
            assert_eq!(owners[first], cells[1]);
            let second = points
                .iter()
                .position(|p| (p[0] - 200.0).abs() < 0.01)
                .unwrap();
            assert_eq!(owners[second], cells[if gap { 1 } else { 2 }]);
            assert_eq!(owners.last(), Some(&cells[2]));
        }
    }

    fn at_hour(hour: f64) -> GameClock {
        GameClock {
            real_seconds: 0.0,
            game_days: 3.0 + hour / 24.0,
            timescale: 20.0,
        }
    }

    #[test]
    fn schedules_use_saved_date_and_hour_independently_of_elapsed_days() {
        let clock = at_hour(3.0);
        let calendar = crate::clock::Calendar {
            year: 205,
            month: 11,
            day: 31,
            hour: 23.5,
        };
        let schedule = Schedule {
            month: 11,
            date: 31,
            hour: 23,
            duration_minutes: 60,
            ..Default::default()
        };
        assert!(schedule_allows_calendar(&schedule, &clock, calendar));
        assert!(!schedule_allows(&schedule, &clock));
    }

    #[test]
    fn a_schedule_window_wraps_past_midnight() {
        let s = Schedule {
            hour: 22,
            duration_minutes: 480,
            ..Default::default()
        };
        assert!(schedule_allows(&s, &at_hour(23.0)));
        assert!(schedule_allows(&s, &at_hour(3.0)));
        assert!(!schedule_allows(&s, &at_hour(8.0)));
        assert!(!schedule_allows(&s, &at_hour(21.9)));
        let open_ended = Schedule {
            hour: 6,
            ..Default::default()
        };
        assert!(schedule_allows(&open_ended, &at_hour(23.0)));
        assert!(!schedule_allows(&open_ended, &at_hour(5.0)));
        assert!(schedule_allows(&Schedule::default(), &at_hour(5.0)));
    }

    #[test]
    fn a_schedule_day_of_week_counts_from_sundas() {
        let monday = Schedule {
            day_of_week: 1,
            ..Default::default()
        };
        let mut c = at_hour(12.0);
        c.game_days = 1.5;
        assert!(schedule_allows(&monday, &c));
        c.game_days = 2.5;
        assert!(!schedule_allows(&monday, &c));
        let weekends = Schedule {
            day_of_week: 8,
            ..Default::default()
        };
        c.game_days = 6.5;
        assert!(schedule_allows(&weekends, &c));
        c.game_days = 7.5;
        assert!(schedule_allows(&weekends, &c));
        c.game_days = 8.5;
        assert!(!schedule_allows(&weekends, &c));
    }

    #[test]
    fn package_speeds_select_the_race_walk_or_run_rate() {
        assert_eq!(movement_input(speed::WALK), (WKMV, 16, WALK_SPEED));
        assert_eq!(
            movement_input(speed::FAST_WALK),
            (WKMV, 20, FAST_WALK_SPEED)
        );
        assert_eq!(movement_input(speed::JOG), (RNMV, 16, JOG_SPEED));
        assert_eq!(movement_input(speed::RUN), (RNMV, 20, RUN_SPEED));
    }

    #[test]
    fn invalid_movement_speeds_are_rejected() {
        let mut data = vec![0; 24];
        data[16..20].copy_from_slice(&123.0f32.to_le_bytes());
        assert_eq!(speed_at(&data, 16), Some(123.0));
        for speed in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            data[16..20].copy_from_slice(&speed.to_le_bytes());
            assert_eq!(speed_at(&data, 16), None);
        }
        assert_eq!(speed_at(&data[..19], 16), None);
    }

    #[test]
    fn ai_state_round_trips() {
        let mut a = AiState {
            package: Some(GlobalFormId { file: 1, object: 2 }),
            alias: Some((GlobalFormId { file: 1, object: 3 }, 4)),
            procedure: Procedure::Unsupported("Patrol".into()),
            tree: Some(TreeState {
                owner: GlobalFormId { file: 1, object: 8 },
                branch: 2,
                active: true,
                finished: false,
            }),
            destination: Some([1.0, 2.0, 3.0]),
            path: vec![[4.0, 5.0, 6.0]],
            path_cells: vec![Some(GlobalFormId { file: 0, object: 9 })],
            arrived: true,
            force_greet_started: false,
            activate_started: false,
            dialogue_activate_started: false,
            found_reference: Some(GlobalFormId {
                file: 1,
                object: 10,
            }),
            sleeping: true,
            bed: None,
            center: Some([7.0, 8.0, 9.0]),
            center_cell: None,
            radius: 100.0,
            dwell_until: 12.5,
            wait_until: None,
            hover_height: 0.0,
            patrol: None,
            speed: 180.0,
            start_position: [0.5, 0.25, 0.125],
            reevaluate: true,
            last_evaluation: 3.0,
            completed: BTreeMap::new(),
        };
        a.completed.insert(GlobalFormId { file: 1, object: 2 }, 5);
        let mut w = Writer::new();
        write_state(&mut w, &a);
        let mut r = Reader::new(&w.out);
        let back = read_state(&mut r).unwrap();
        r.expect_end().unwrap();
        assert_eq!(back, a);
    }
    #[test]
    fn an_actor_record_stops_before_the_next_actor_field() {
        let state = AiState {
            package: Some(GlobalFormId { file: 1, object: 2 }),
            ..Default::default()
        };
        let mut writer = Writer::new();
        write_state(&mut writer, &state);
        writer.u32(0x12345678);
        let mut reader = Reader::new(&writer.out);
        assert_eq!(read_state(&mut reader).unwrap(), state);
        assert_eq!(reader.u32().unwrap(), 0x12345678);
        reader.expect_end().unwrap();
    }
}

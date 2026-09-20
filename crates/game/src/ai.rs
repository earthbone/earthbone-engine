// SPDX-License-Identifier: GPL-3.0-or-later

//! Package evaluation and the actor driver. An actor's package stack is its
//! quest alias packages (running quests first, by priority) followed by the
//! `PKID` list of its base record, and the first package whose schedule and
//! conditions hold wins, the way the Creation Kit wiki describes the stack
//! and `EvaluatePackage` (`engine-refs/ck-wiki/EvaluatePackage_-_Actor`).
//! The chosen package's template names a procedure, and three of them move
//! the actor on the clock: `Travel` walks to a place, `Sandbox` wanders
//! between idle markers around a place, `Sleep` walks to a bed and holds.
//! Walking follows the vanilla navmesh graph (`soe_navmesh::vanilla`)
//! built once per world, and crossing into another navmesh moves the actor
//! into that navmesh's cell.
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
use soe_papyrus::VmError;

use crate::clock::GameClock;
use crate::package::{
    condition, flags, speed, CompValue, Condition, DataValue, Location, LocationData, Package,
    Schedule,
};
use crate::world::save::{read_form_id, read_opt_form_id, FormNamespace, FormReader};
use crate::world::{Alloc, Handle, Pending, World};

/// Units per real second, the default movement type's walk, jog, run and fast
/// walk speeds. Race movement data is not read yet.
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

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// What the chosen package makes the actor do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Procedure {
    #[default]
    None,
    Travel,
    Sandbox,
    Sleep,
    /// A procedure the driver does not run yet, by its template name. The
    /// actor holds where it stands.
    Unsupported(String),
}

/// Per-actor driver state, saved with the actor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiState {
    pub package: Option<GlobalFormId>,
    /// The quest alias the package came from, none for a base package.
    pub alias: Option<(GlobalFormId, u32)>,
    pub procedure: Procedure,
    pub destination: Option<[f32; 3]>,
    /// Waypoints still ahead, first to reach first.
    pub path: Vec<[f32; 3]>,
    /// The cell the actor is in once it passes the matching waypoint.
    pub path_cells: Vec<Option<GlobalFormId>>,
    pub arrived: bool,
    pub sleeping: bool,
    pub bed: Option<GlobalFormId>,
    pub center: Option<[f32; 3]>,
    pub center_cell: Option<GlobalFormId>,
    pub radius: f32,
    /// Game hours until a sandboxing actor leaves its marker.
    pub dwell_until: f64,
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

/// Actor value names by the index a `GetActorValue` condition carries.
const ACTOR_VALUES: [&str; 30] = [
    "aggression",
    "confidence",
    "energy",
    "morality",
    "mood",
    "assistance",
    "onehanded",
    "twohanded",
    "marksman",
    "block",
    "smithing",
    "heavyarmor",
    "lightarmor",
    "pickpocket",
    "lockpicking",
    "sneak",
    "alchemy",
    "speechcraft",
    "alteration",
    "conjuration",
    "destruction",
    "illusion",
    "restoration",
    "enchanting",
    "health",
    "magicka",
    "stamina",
    "healrate",
    "magickarate",
    "staminarate",
];

// ----- views -----

/// The `PACK` view of a winning record.
pub fn package(world: &World, id: GlobalFormId) -> Option<Package> {
    let rec = world.record(id)?;
    Package::from_record(rec).ok()
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

/// What the package makes an actor do, and the location input it reads.
/// The tree's branch structure is not walked. Most vanilla templates are a
/// sequence of a `Travel` to the place followed by what happens there, so
/// among the procedures the driver knows the one that happens there wins:
/// `Sleep` over `Sandbox` over `Travel`. The driver walks to the place on
/// its own for each of them.
pub fn procedure_of(world: &World, id: GlobalFormId) -> (Procedure, Option<LocationData>, bool) {
    let chain = template_chain(world, id);
    let mut first_unknown: Option<String> = None;
    let mut best: Option<(u8, Procedure, Option<LocationData>, bool)> = None;
    for (_, template) in &chain {
        for branch in template.procedures() {
            let Some(name) = branch.procedure.as_deref() else {
                continue;
            };
            let (rank, procedure) = match name.to_ascii_lowercase().as_str() {
                "travel" => (1, Procedure::Travel),
                "sandbox" => (2, Procedure::Sandbox),
                "sleep" => (3, Procedure::Sleep),
                _ => {
                    if first_unknown.is_none() {
                        first_unknown = Some(name.to_string());
                    }
                    continue;
                }
            };
            if best.as_ref().is_some_and(|(r, ..)| *r >= rank) {
                continue;
            }
            let location = branch.inputs.iter().find_map(|&index| {
                chain
                    .iter()
                    .find_map(|(_, p)| match p.input(index as i8).map(|i| i.value) {
                        Some(DataValue::Location(l)) => Some(l),
                        _ => None,
                    })
            });
            let location = location.or_else(|| first_location(&chain));
            best = Some((rank, procedure, location, branch.success_completes_package));
        }
    }
    match (best, first_unknown) {
        (Some((_, procedure, location, completes)), _) => (procedure, location, completes),
        (None, Some(name)) => (Procedure::Unsupported(name), None, false),
        (None, None) => (Procedure::None, None, false),
    }
}

fn first_location(chain: &[(GlobalFormId, Package)]) -> Option<LocationData> {
    chain.iter().find_map(|(_, p)| {
        p.inputs.iter().find_map(|i| match i.value {
            DataValue::Location(l) => Some(l),
            _ => None,
        })
    })
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
    if let Ok((source, npc)) = world.actor_npc_part(actor, crate::npc::template::AI_PACKAGES) {
        for raw in &npc.packages {
            if let Some(package) = world.resolve_from(source, *raw) {
                out.push(Candidate {
                    package,
                    alias: None,
                });
            }
        }
    }
    out
}

/// The first candidate whose schedule and conditions hold. A running
/// `Must Complete` package that still holds keeps its place.
pub fn select(world: &mut World, actor: GlobalFormId) -> Option<Candidate> {
    world.ai.evaluations += 1;
    let today = world.clock.game_days.floor() as u32;
    let completed = world.actor(actor)?.ai.completed.clone();
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
        14 => {
            let name = ACTOR_VALUES.get(c.param1 as usize)?;
            world.actor_value(subject, name)
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
            let name = ACTOR_VALUES.get(c.param1 as usize)?;
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
    let object = world.object_of(Handle::Form(actor), alloc);
    if let Some(old) = current {
        let old_value = world.form_value(Some(old), alloc);
        let name = if chosen.is_some() {
            "OnPackageChange"
        } else {
            "OnPackageEnd"
        };
        world.pending.push(Pending::Event {
            object,
            name: name.into(),
            args: vec![old_value],
        });
    }
    let (position, cell) = world
        .reference(actor)
        .map(|r| (r.position, r.cell))
        .unwrap_or_default();
    let mut ai = std::mem::take(&mut world.actor(actor)?.ai);
    ai.package = chosen.map(|c| c.package);
    ai.alias = chosen.and_then(|c| c.alias);
    ai.procedure = Procedure::None;
    ai.destination = None;
    ai.path.clear();
    ai.path_cells.clear();
    ai.arrived = false;
    ai.sleeping = false;
    ai.bed = None;
    ai.center = None;
    ai.center_cell = None;
    ai.radius = 0.0;
    ai.dwell_until = 0.0;
    ai.speed = WALK_SPEED;
    ai.start_position = position;
    if let Some(c) = chosen {
        let (procedure, location, _) = procedure_of(world, c.package);
        ai.procedure = procedure;
        if let Some(p) = package(world, c.package) {
            ai.speed = if p.has_flag(flags::PREFERRED_SPEED) {
                match p.general.preferred_speed {
                    speed::JOG => JOG_SPEED,
                    speed::RUN => RUN_SPEED,
                    speed::FAST_WALK => FAST_WALK_SPEED,
                    _ => WALK_SPEED,
                }
            } else {
                WALK_SPEED
            };
        }
        let place =
            location.and_then(|l| resolve_location(world, actor, c.package, c.alias, &ai, &l));
        match ai.procedure {
            Procedure::Travel => {
                ai.destination = place.map(|p| p.position);
                ai.center_cell = place.and_then(|p| p.cell);
            }
            Procedure::Sandbox => {
                let center = place.map_or(position, |p| p.position);
                ai.center = Some(center);
                ai.center_cell = place.and_then(|p| p.cell).or(cell);
                ai.radius = location.map_or(0.0, |l| l.radius as f32);
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
            Procedure::None | Procedure::Unsupported(_) => {}
        }
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
        Location::AliasLocation(_) | Location::Other(..) => None,
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
        let Some(a) = world.actor(actor) else {
            continue;
        };
        if a.dead || a.unconscious {
            continue;
        }
        let due = a.ai.package.is_none()
            || a.ai.reevaluate
            || now - a.ai.last_evaluation >= EVALUATION_INTERVAL;
        if due {
            evaluate(world, actor, alloc);
        }
        step(world, actor, dt as f32);
    }
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
) -> (Vec<[f32; 3]>, Vec<Option<GlobalFormId>>) {
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

fn step(world: &mut World, actor: GlobalFormId, dt: f32) {
    let Some((position, cell)) = world.reference(actor).map(|r| (r.position, r.cell)) else {
        return;
    };
    let Some(mut ai) = world.actor(actor).map(|a| std::mem::take(&mut a.ai)) else {
        return;
    };
    let hours = world.clock.game_hours();
    if ai.arrived {
        match ai.procedure {
            Procedure::Sandbox => {
                if hours >= ai.dwell_until {
                    ai.arrived = false;
                    ai.destination = None;
                }
            }
            Procedure::Sleep => ai.sleeping = true,
            _ => {}
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
            if d <= budget.max(ARRIVAL_DISTANCE) {
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
            if ai.procedure == Procedure::Travel {
                if let Some(p) = ai.package {
                    let today = world.clock.game_days.floor() as u32;
                    ai.completed.insert(p, today);
                }
            }
        }
        if let Some(r) = world.reference(actor) {
            r.position = pos;
            if new_cell.is_some() {
                r.cell = new_cell;
            }
        }
    }
    if let Some(a) = world.actor(actor) {
        a.ai = ai;
    }
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

pub fn write_state(w: &mut Writer, a: &AiState) {
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
        Procedure::None => w.u8(0),
        Procedure::Travel => w.u8(1),
        Procedure::Sandbox => w.u8(2),
        Procedure::Sleep => w.u8(3),
        Procedure::Unsupported(name) => {
            w.u8(4);
            w.str(name);
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
}

pub fn read_state(r: &mut Reader<'_>) -> Result<AiState, VmError> {
    read_state_mapped(&mut FormReader::new(r, FormNamespace::Current))
}

pub(crate) fn read_state_mapped(r: &mut FormReader<'_, '_>) -> Result<AiState, VmError> {
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
        4 => Procedure::Unsupported(r.str()?),
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
    Ok(a)
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
    fn ai_state_round_trips() {
        let mut a = AiState {
            package: Some(GlobalFormId { file: 1, object: 2 }),
            alias: Some((GlobalFormId { file: 1, object: 3 }, 4)),
            procedure: Procedure::Unsupported("Patrol".into()),
            destination: Some([1.0, 2.0, 3.0]),
            path: vec![[4.0, 5.0, 6.0]],
            path_cells: vec![Some(GlobalFormId { file: 0, object: 9 })],
            arrived: true,
            sleeping: true,
            bed: None,
            center: Some([7.0, 8.0, 9.0]),
            center_cell: None,
            radius: 100.0,
            dwell_until: 12.5,
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
}

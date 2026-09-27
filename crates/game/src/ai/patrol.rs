// SPDX-License-Identifier: GPL-3.0-or-later

//! Walking linked-marker Patrol, from xEdit XLKR/XPRD and UNI's Patrol tests.
//! Route definitions remain on winning references. Only traversal progress is saved.

use super::{distance, tree, AiState, Procedure, World};
use crate::package::{DataValue, Target};
use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[derive(Debug, Clone, PartialEq)]
pub struct PatrolState {
    pub start: GlobalFormId,
    pub current: GlobalFormId,
    pub reverse: bool,
    pub dwell_until: Option<f64>,
}

pub(super) struct Parameters {
    owner: GlobalFormId,
    target: Target,
    radius: f32,
    repeat: bool,
    nearest: bool,
}

impl Parameters {
    pub(super) fn from_inputs(inputs: &[(GlobalFormId, DataValue)]) -> Result<Self, &'static str> {
        let [(owner, DataValue::Target(target)), (_, DataValue::Float(radius)), (_, DataValue::Bool(repeat)), (_, DataValue::Bool(nearest)), (_, DataValue::Bool(horse)), (_, DataValue::Bool(static_path))] =
            inputs
        else {
            return Err("Patrol parameter types or omitted inputs, AI-90");
        };
        if *horse {
            return Err("mounted Patrol, AI-89");
        }
        if *static_path {
            return Err("Patrol StaticPathing, AI-90");
        }
        if !radius.is_finite() || *radius < 0.0 {
            return Err("invalid Patrol radius");
        }
        if !matches!(
            target.target,
            Target::Reference(_) | Target::LinkedReference(0) | Target::Alias(_)
        ) {
            return Err("Patrol target mode, AI-91");
        }
        Ok(Self {
            owner: *owner,
            target: target.target,
            radius: *radius,
            repeat: *repeat,
            nearest: *nearest,
        })
    }
}

fn next_link(
    world: &mut World,
    marker: GlobalFormId,
) -> Result<Option<GlobalFormId>, &'static str> {
    let reference = world.reference(marker).ok_or("missing Patrol reference")?;
    let mut links = reference
        .linked
        .iter()
        .filter(|(keyword, _)| keyword.is_none());
    let target = links.next().map(|(_, target)| *target);
    if links.next().is_some() {
        return Err("ambiguous Patrol links without a keyword");
    }
    Ok(target)
}

fn marker(
    world: &mut World,
    id: GlobalFormId,
) -> Result<([f32; 3], Option<GlobalFormId>, f32), &'static str> {
    let rec = world.record(id).ok_or("missing Patrol marker")?;
    if rec.tag != T(b"REFR") || rec.is_deleted() {
        return Err("invalid Patrol marker record");
    }
    let mut dwell = None;
    let mut in_patrol = false;
    for sub in &rec.subrecords {
        match sub.tag.as_bytes() {
            b"XPRD" => {
                if dwell.is_some() {
                    return Err("multiple Patrol marker data groups");
                }
                let bytes = sub
                    .data
                    .as_slice()
                    .try_into()
                    .map_err(|_| "invalid Patrol idle time")?;
                dwell = Some(f32::from_le_bytes(bytes));
                in_patrol = true;
            }
            b"XPPA" if in_patrol => {
                if !sub.data.is_empty() {
                    return Err("invalid Patrol script marker");
                }
            }
            b"INAM" if in_patrol => {
                if sub.data != [0; 4] {
                    return Err("Patrol marker idle, AI-88");
                }
            }
            b"PDTO" if in_patrol => {
                if sub.data != [0; 8] {
                    return Err("Patrol marker topic, AI-88");
                }
            }
            b"SCHR" | b"SCTX" if in_patrol => {}
            _ => in_patrol = false,
        }
    }
    let dwell = dwell.ok_or("Patrol marker has no idle time")?;
    if !dwell.is_finite() || dwell < 0.0 {
        return Err("invalid Patrol idle time");
    }
    let reference = world.reference(id).ok_or("missing Patrol marker state")?;
    if reference.deleted || !reference.enabled || !reference.position.iter().all(|v| v.is_finite())
    {
        return Err("unavailable Patrol marker");
    }
    let result = (reference.position, reference.cell, dwell);
    let base = reference.base;
    if base
        .and_then(|id| world.tag_of(id))
        .is_some_and(|tag| tag == T(b"FURN") || tag == T(b"IDLM"))
    {
        return Err("Patrol furniture or idle marker use, AI-88");
    }
    Ok(result)
}

fn route(
    world: &mut World,
    start: GlobalFormId,
) -> Result<(Vec<GlobalFormId>, bool), &'static str> {
    let mut points = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    let mut next = start;
    loop {
        if !visited.insert(next) {
            return if next == start {
                Ok((points, true))
            } else {
                Err("Patrol route has a non-root cycle")
            };
        }
        marker(world, next)?;
        points.push(next);
        match next_link(world, next)? {
            Some(id) => next = id,
            None => return Ok((points, false)),
        }
    }
}

fn next_index(
    index: usize,
    len: usize,
    cyclic: bool,
    repeat: bool,
    reverse: &mut bool,
) -> Option<usize> {
    if cyclic {
        return Some((index + 1) % len);
    }
    if *reverse && index > 0 {
        Some(index - 1)
    } else if !*reverse && index + 1 < len {
        Some(index + 1)
    } else if repeat && len > 1 {
        *reverse = !*reverse;
        Some(if *reverse { index - 1 } else { index + 1 })
    } else {
        None
    }
}

fn destination(
    world: &mut World,
    ai: &mut AiState,
    current: GlobalFormId,
) -> Result<(), &'static str> {
    let (position, cell, _) = marker(world, current)?;
    ai.destination = Some(position);
    ai.center_cell = cell;
    ai.arrived = false;
    ai.path.clear();
    ai.path_cells.clear();
    Ok(())
}

fn fail(ai: &mut AiState, reason: &'static str) {
    ai.procedure = Procedure::Unsupported(reason.into());
    ai.patrol = None;
    ai.destination = None;
    ai.path.clear();
    ai.path_cells.clear();
}

pub(super) fn begin(
    world: &mut World,
    actor: GlobalFormId,
    ai: &mut AiState,
    params: Result<Parameters, &'static str>,
) {
    if let Err(reason) = start(world, actor, ai, params) {
        fail(ai, reason);
    }
}

fn start(
    world: &mut World,
    actor: GlobalFormId,
    ai: &mut AiState,
    params: Result<Parameters, &'static str>,
) -> Result<(), &'static str> {
    let params = params?;
    let start = match params.target {
        Target::Reference(raw) if raw != 0 => world.resolve_from(params.owner, raw),
        Target::LinkedReference(0) => next_link(world, actor)?,
        Target::Alias(alias) => ai
            .package
            .and_then(|package| world.owning_quest(package))
            .and_then(|quest| world.alias_ref(quest, alias)),
        _ => None,
    }
    .ok_or("Patrol has no route start")?;
    let (points, cyclic) = route(world, start)?;
    if points.len() == 1 && params.repeat && !cyclic {
        return Err("single-marker repeating Patrol has no return leg");
    }
    let mut index = 0;
    let mut reverse = false;
    let mut used_terminal = false;
    if params.nearest {
        let position = world
            .reference(actor)
            .ok_or("missing Patrol actor")?
            .position;
        let mut best = f32::INFINITY;
        for (i, point) in points.iter().enumerate() {
            let d = distance(position, marker(world, *point)?.0);
            if d < best {
                best = d;
                index = i;
            }
        }
        match next_index(index, points.len(), cyclic, params.repeat, &mut reverse) {
            Some(next) => index = next,
            None => used_terminal = true,
        }
    }
    ai.radius = params.radius;
    destination(world, ai, points[index])?;
    ai.patrol = Some(PatrolState {
        start,
        current: points[index],
        reverse,
        dwell_until: used_terminal.then_some(world.clock.real_seconds),
    });
    ai.arrived = used_terminal;
    Ok(())
}

pub(super) fn tick(world: &mut World, ai: &mut AiState) {
    if !ai.tree.as_ref().is_some_and(|tree| tree.active) {
        return;
    }
    if let Err(reason) = advance(world, ai) {
        fail(ai, reason);
    }
}

pub(super) fn prepare(world: &mut World, ai: &mut AiState) {
    let result = ai
        .patrol
        .as_ref()
        .map(|state| (state.start, state.current))
        .ok_or("missing Patrol progress")
        .and_then(|(start, current)| {
            if !route(world, start)?.0.contains(&current) {
                return Err("Patrol marker left its route");
            }
            let (position, cell, _) = marker(world, current)?;
            if ai.destination != Some(position) || ai.center_cell != cell {
                destination(world, ai, current)?;
                if let Some(state) = &mut ai.patrol {
                    state.dwell_until = None;
                }
            }
            Ok(())
        });
    if let Err(reason) = result {
        fail(ai, reason);
    }
}

fn advance(world: &mut World, ai: &mut AiState) -> Result<(), &'static str> {
    let mut state = ai.patrol.clone().ok_or("missing Patrol progress")?;
    let (position, _, dwell) = marker(world, state.current)?;
    if !ai.arrived {
        if ai.destination != Some(position) {
            destination(world, ai, state.current)?;
        }
        return Ok(());
    }
    let now = world.clock.real_seconds;
    if !now.is_finite() {
        return Err("invalid Patrol clock");
    }
    let Some(until) = state.dwell_until else {
        state.dwell_until = Some(now + f64::from(dwell));
        ai.patrol = Some(state);
        return Ok(());
    };
    if now < until {
        return Ok(());
    }
    let params = tree::patrol_parameters(world, ai)?;
    let (points, cyclic) = route(world, state.start)?;
    let index = points
        .iter()
        .position(|id| *id == state.current)
        .ok_or("Patrol marker left its route")?;
    if let Some(next) = next_index(
        index,
        points.len(),
        cyclic,
        params.repeat,
        &mut state.reverse,
    ) {
        state.current = points[next];
        state.dwell_until = None;
        ai.radius = params.radius;
        destination(world, ai, state.current)?;
        ai.patrol = Some(state);
    } else {
        tree::succeeded(world, ai);
    }
    Ok(())
}

pub(super) fn write(w: &mut Writer, state: Option<&PatrolState>) {
    w.bool(state.is_some());
    if let Some(state) = state {
        for id in [state.start, state.current] {
            w.u16(id.file);
            w.u32(id.object);
        }
        w.bool(state.reverse);
        w.bool(state.dwell_until.is_some());
        if let Some(until) = state.dwell_until {
            w.f64(until);
        }
    }
}

pub(super) fn read(r: &mut Reader<'_>) -> Result<PatrolState, VmError> {
    if !r.bool()? {
        return Err(VmError::State("missing Patrol progress".into()));
    }
    let start = GlobalFormId {
        file: r.u16()?,
        object: r.u32()?,
    };
    let current = GlobalFormId {
        file: r.u16()?,
        object: r.u32()?,
    };
    let reverse = r.bool()?;
    let dwell_until = if r.bool()? {
        let until = r.f64()?;
        if !until.is_finite() || until < 0.0 {
            return Err(VmError::State("invalid Patrol dwell deadline".into()));
        }
        Some(until)
    } else {
        None
    };
    Ok(PatrolState {
        start,
        current,
        reverse,
        dwell_until,
    })
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Walking linked-marker Patrol, from xEdit XLKR/XPRD, Mutagen's Patrol fields,
//! and UNI's Patrol tests.
//! Route definitions remain on winning references. Only traversal progress is saved.

use super::{distance, tree, AiState, Procedure, World};
use crate::package::{DataValue, Target};
use crate::world::Alloc;
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
            Target::Reference(_) | Target::LinkedReference(0)
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

fn marker(world: &mut World, id: GlobalFormId) -> Result<Marker, &'static str> {
    let rec = world.record(id).ok_or("missing Patrol marker")?;
    if rec.tag != T(b"REFR") || rec.is_deleted() {
        return Err("invalid Patrol marker record");
    }
    let mut dwell = None;
    let mut idle = None;
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
                let raw = sub
                    .data
                    .as_slice()
                    .try_into()
                    .map_err(|_| "invalid Patrol marker idle")?;
                let raw = u32::from_le_bytes(raw);
                if raw != 0 {
                    let form = world
                        .resolve_from(id, raw)
                        .ok_or("missing Patrol marker idle")?;
                    if world.tag_of(form) != Some(T(b"IDLE")) {
                        return Err("invalid Patrol marker idle form");
                    }
                    idle = Some(form);
                }
            }
            b"PDTO" if in_patrol => {
                validate_topic(world, id, &sub.data)?;
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
    Ok(Marker {
        position: reference.position,
        cell: reference.cell,
        dwell,
        idle,
        furniture: reference
            .base
            .is_some_and(|base| world.tag_of(base) == Some(T(b"FURN"))),
    })
}

struct Marker {
    position: [f32; 3],
    cell: Option<GlobalFormId>,
    dwell: f32,
    idle: Option<GlobalFormId>,
    furniture: bool,
}

fn validate_topic(world: &World, marker: GlobalFormId, data: &[u8]) -> Result<(), &'static str> {
    let kind = data.get(..4).ok_or("invalid Patrol marker topic")?;
    let kind = u32::from_le_bytes(kind.try_into().map_err(|_| "invalid Patrol marker topic")?);
    let payload = data.get(4..).ok_or("invalid Patrol marker topic")?;
    match kind {
        0 => {
            let raw = u32::from_le_bytes(
                payload
                    .try_into()
                    .map_err(|_| "invalid Patrol marker topic reference")?,
            );
            if raw != 0 {
                let topic = world
                    .resolve_from(marker, raw)
                    .ok_or("missing Patrol marker topic")?;
                if world.tag_of(topic) != Some(T(b"DIAL")) {
                    return Err("invalid Patrol marker topic form");
                }
            }
        }
        1 if payload.len() == 4 => {}
        _ => return Err("invalid Patrol marker topic type"),
    }
    Ok(())
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
    let marker = marker(world, current)?;
    ai.destination = Some(marker.position);
    ai.center_cell = marker.cell;
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
            let d = distance(position, marker(world, *point)?.position);
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

pub(super) fn tick(world: &mut World, actor: GlobalFormId, ai: &mut AiState, alloc: Alloc<'_>) {
    if !ai.tree.as_ref().is_some_and(|tree| tree.active) {
        return;
    }
    if let Err(reason) = advance(world, actor, ai, alloc) {
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
            let marker = marker(world, current)?;
            if ai.destination != Some(marker.position) || ai.center_cell != marker.cell {
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

fn advance(
    world: &mut World,
    actor: GlobalFormId,
    ai: &mut AiState,
    alloc: Alloc<'_>,
) -> Result<(), &'static str> {
    let mut state = ai.patrol.clone().ok_or("missing Patrol progress")?;
    let marker = marker(world, state.current)?;
    if !ai.arrived {
        if ai.destination != Some(marker.position) {
            destination(world, ai, state.current)?;
        }
        return Ok(());
    }
    let now = world.clock.real_seconds;
    if !now.is_finite() {
        return Err("invalid Patrol clock");
    }
    let Some(until) = state.dwell_until else {
        if marker.furniture && !world.activate_furniture(state.current, actor) {
            return Err("Patrol furniture marker unavailable");
        }
        if let Some(idle) = marker.idle {
            let name = world.editor_id(idle).unwrap_or_default();
            world.animation_event(actor, &name, alloc);
        }
        state.dwell_until = Some(now + f64::from(marker.dwell));
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
        release_furniture(world, actor, state.current);
        state.current = points[next];
        state.dwell_until = None;
        ai.radius = params.radius;
        destination(world, ai, state.current)?;
        ai.patrol = Some(state);
    } else {
        release_furniture(world, actor, state.current);
        tree::succeeded(world, ai);
    }
    Ok(())
}

fn release_furniture(world: &mut World, actor: GlobalFormId, marker: GlobalFormId) {
    if world
        .furniture_seat(actor)
        .is_some_and(|(furniture, _)| furniture == marker)
    {
        world.stand_from_furniture(actor);
    }
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Gameplay CTDA execution. Field/function identities come from xEdit
//! wbConditions/wbConditionFunctions. CK documents the state queries and
//! conditional Papyrus variables. See docs/game.md for the implemented set.

use crate::package::DataValue;
use crate::{inventory::Field, Handle, World};
use soe_esm::{
    records::gameplay::{Comparison, Condition},
    records::refr::Refr,
    GlobalFormId, Tag,
};
use soe_papyrus::{Value, Vm};

mod actor_values;

/// The record supplies local FormID mapping. Subject and target are runtime forms.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub owner: GlobalFormId,
    pub subject: GlobalFormId,
    pub target: Option<GlobalFormId>,
    pub quest: Option<GlobalFormId>,
    pub tempering_enchanted: Option<bool>,
}

/// A condition could not be evaluated. It must not be treated as true.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("unsupported condition function {0}")]
    Function(u16),
    #[error("invalid actor-value index {0}")]
    ActorValue(u32),
    #[error("unsupported second parameter {1:#x} for condition function {0}")]
    Parameter(u16, u32),
    #[error("unsupported condition flags {0:#x}")]
    Flags(u8),
    #[error("unsupported or unavailable condition run-on target {0}")]
    Target(u32),
    #[error("unsupported condition comparison operator {0}")]
    Operator(u8),
    #[error("missing or wrong-type condition form {0}")]
    Form(GlobalFormId),
    #[error("condition variable {0} is absent, ambiguous, nonconditional or nonnumeric")]
    Variable(String),
    #[error("condition has a non-finite numeric value")]
    NonFinite,
    #[error("condition requires tempering item context")]
    TemperingContext,
    #[error("condition requires the subject's current package {0}")]
    PackageContext(GlobalFormId),
    #[error("condition function {0} requires runtime state the engine does not track")]
    Unavailable(u16),
    #[error("condition requires the subject's current location {0}")]
    LocationContext(GlobalFormId),
    #[error("condition count overflow or cyclic form list")]
    Inventory,
}

/// AND groups of OR-linked predicates, evaluated in file order with short circuiting.
///
/// # Errors
///
/// Unsupported predicates and missing required context return an error when reached.
pub fn evaluate(
    world: &mut World,
    vm: &Vm,
    context: Context,
    conditions: &[Condition],
) -> Result<bool, Error> {
    evaluate_with_vm(world, Some(vm), context, conditions)
}

/// Evaluate conditions that do not require a Papyrus variable lookup.
///
/// Direct world actions have no VM borrow while they queue their later script
/// events. Variable-backed predicates return [`Error::Variable`] in this mode.
pub(crate) fn evaluate_without_vm(
    world: &mut World,
    context: Context,
    conditions: &[Condition],
) -> Result<bool, Error> {
    evaluate_with_vm(world, None, context, conditions)
}

fn evaluate_with_vm(
    world: &mut World,
    vm: Option<&Vm>,
    context: Context,
    conditions: &[Condition],
) -> Result<bool, Error> {
    let mut any = false;
    for (index, condition) in conditions.iter().enumerate() {
        if !any {
            any = evaluate_one_with_vm(world, vm, context, condition)?;
        }
        if !condition.or_with_next() || index + 1 == conditions.len() {
            if !any {
                return Ok(false);
            }
            any = false;
        }
    }
    Ok(true)
}

/// Whether this evaluator has a function implementation. Context can still be missing.
pub fn supports_function(function: u16) -> bool {
    matches!(
        function,
        1 | 6
            | 8
            | 10
            | 11
            | 14
            | 25
            | 26
            | 49
            | 81
            | 128
            | 277
            | 408
            | 430
            | 494
            | 18
            | 46
            | 47
            | 56
            | 58
            | 59
            | 67
            | 72
            | 69
            | 70
            | 71
            | 73
            | 80
            | 116
            | 225
            | 315
            | 403
            | 566
            | 74
            | 136
            | 182
            | 264
            | 289
            | 359
            | 448
            | 543
            | 560
            | 606
            | 629
            | 630
            | 655
            | 659
            | 53
            | 79
            | 470
            | 528
            | 230
            | 250
            | 280
            | 310
            | 325
            | 360
            | 444
            | 445
            | 446
            | 491
            | 519
            | 561
            | 562
            | 563
            | 565
            | 567
            | 569
            | 591
            | 592
            | 598
            | 600
            | 601
            | 603
            | 604
            | 605
            | 610
            | 625
            | 626
            | 651
            | 718
            | 185
            | 219
            | 286
            | 287
            | 313
            | 318
            | 327
            | 339
            | 91
            | 98
            | 106
            | 107
            | 110
            | 111
            | 125
            | 127
    )
}

/// Evaluate one predicate, useful for diagnostics independently of list short circuiting.
///
/// # Errors
///
/// Unsupported flags/functions and invalid form or variable references are errors.
pub fn evaluate_one(
    world: &mut World,
    vm: &Vm,
    context: Context,
    c: &Condition,
) -> Result<bool, Error> {
    evaluate_one_with_vm(world, Some(vm), context, c)
}

fn evaluate_one_with_vm(
    world: &mut World,
    vm: Option<&Vm>,
    context: Context,
    c: &Condition,
) -> Result<bool, Error> {
    if c.flags & 0x0a != 0 {
        return Err(Error::Flags(c.flags & 0x1f));
    }
    let (context_subject, context_target) = if c.flags & 0x10 != 0 {
        (context.target, Some(context.subject))
    } else {
        (Some(context.subject), context.target)
    };
    let subject = match c.run_on {
        0 => context_subject,
        1 => context_target,
        2 => Some(resolve_reference(world, context.owner, c.reference)?),
        5 => context.quest.and_then(|quest| {
            u32::try_from(c.parameter_3)
                .ok()
                .and_then(|alias| world.alias_ref(quest, alias))
        }),
        target => return Err(Error::Target(target)),
    }
    .ok_or(Error::Target(c.run_on))?;
    let actual = function_value(world, vm, context, subject, c)?;
    let expected = match c.comparison {
        Comparison::Number(value) => value,
        Comparison::Global(raw) => {
            let global = resolve(world, context.owner, raw, Some(*b"GLOB"))?;
            world.global_value(global)
        }
    };
    if !actual.is_finite() || !expected.is_finite() {
        return Err(Error::NonFinite);
    }
    Ok(match c.operator() {
        0 => actual == expected,
        1 => actual != expected,
        2 => actual > expected,
        3 => actual >= expected,
        4 => actual < expected,
        5 => actual <= expected,
        op => return Err(Error::Operator(op)),
    })
}

fn boolean(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn location_ref_type_count(
    world: &mut World,
    location: GlobalFormId,
    ref_type: GlobalFormId,
    dead_count: bool,
) -> Result<f32, Error> {
    let record = world.record(location).ok_or(Error::Form(location))?;
    let mut removed = Vec::new();
    for entry in record.all(Tag(*b"RCSR")) {
        let (items, remainder) = entry.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err(Error::Form(location));
        }
        for item in items {
            let raw_ref = u32::from_le_bytes(*item);
            if let Some(reference) = world.resolve_from(location, raw_ref) {
                removed.push(reference);
            }
        }
    }
    let mut references = Vec::new();
    for entry in record.all(Tag(*b"ACSR")).chain(record.all(Tag(*b"LCSR"))) {
        let (items, remainder) = entry.as_chunks::<16>();
        if !remainder.is_empty() {
            return Err(Error::Form(location));
        }
        for item in items {
            let raw_type =
                u32::from_le_bytes(item[0..4].try_into().map_err(|_| Error::Form(location))?);
            let raw_ref =
                u32::from_le_bytes(item[4..8].try_into().map_err(|_| Error::Form(location))?);
            if world.resolve_from(location, raw_type) == Some(ref_type) {
                if let Some(reference) = world
                    .resolve_from(location, raw_ref)
                    .filter(|reference| !removed.contains(reference))
                {
                    references.push(reference);
                }
            }
        }
    }
    references.sort_unstable();
    references.dedup();
    let mut count = 0_u32;
    for reference in references {
        if world
            .actor(reference)
            .is_some_and(|actor| actor.dead == dead_count)
        {
            count = count.checked_add(1).ok_or(Error::Inventory)?;
        }
    }
    Ok(count as f32)
}

/// Rejects a value in either parameter slot of a function that takes none,
/// the same way functions 70 and 448 reject a slot they do not use.
fn no_parameter(c: &Condition) -> Result<(), Error> {
    match c.parameters {
        [0, 0] => Ok(()),
        [0, other] | [other, _] => Err(Error::Parameter(c.function, other)),
    }
}

fn no_second_parameter(c: &Condition) -> Result<(), Error> {
    if c.parameters[1] == 0 {
        Ok(())
    } else {
        Err(Error::Parameter(c.function, c.parameters[1]))
    }
}

/// Mutagen Skyrim.Constants.Player is reserved even without a placed ACHR record.
fn resolve_reference(
    world: &mut World,
    owner: GlobalFormId,
    raw: u32,
) -> Result<GlobalFormId, Error> {
    let mut id = world.resolve_from(owner, raw).ok_or(Error::Form(owner))?;
    if id
        == (GlobalFormId {
            file: 0,
            object: 0x14,
        })
    {
        id = world.player();
    }
    if raw == 0
        || world
            .reference(id)
            .is_none_or(|reference| reference.deleted)
    {
        return Err(Error::Form(id));
    }
    Ok(id)
}

fn reference_worldspace(world: &mut World, reference: GlobalFormId) -> Option<GlobalFormId> {
    let cell = world.reference(reference)?.cell?;
    let raw = world.record(cell)?.placement.world?;
    world.resolve_from(cell, raw)
}

fn package_location_condition(
    world: &mut World,
    subject: GlobalFormId,
    c: &Condition,
    index: u32,
) -> Result<f32, Error> {
    let package_id = world
        .current_package(subject)
        .ok_or(Error::PackageContext(subject))?;
    let ai_state = world.actor(subject).ok_or(Error::Form(subject))?.ai.clone();
    let index = i8::try_from(index).map_err(|_| Error::Parameter(c.function, index))?;
    let package = crate::ai::package(world, package_id).ok_or(Error::Form(package_id))?;
    let input = package
        .input(index)
        .ok_or(Error::Parameter(c.function, index as u32))?;
    let DataValue::Location(location) = &input.value else {
        return Err(Error::Parameter(c.function, index as u32));
    };
    let Some(place) = crate::ai::resolve_location(
        world,
        subject,
        package_id,
        ai_state.alias,
        &ai_state,
        location,
    ) else {
        return Err(Error::PackageContext(subject));
    };
    let reference = world.reference(subject).ok_or(Error::Form(subject))?;
    let inside = if matches!(location.location, crate::package::Location::InCell(_)) {
        reference.cell == place.cell
    } else {
        let distance_squared = reference
            .position
            .iter()
            .zip(place.position)
            .map(|(a, b)| f64::from(*a - b).powi(2))
            .sum::<f64>();
        distance_squared <= f64::from(location.radius.max(0)).powi(2)
    };
    Ok(boolean(inside))
}

pub(crate) fn resolve(
    world: &World,
    owner: GlobalFormId,
    raw: u32,
    tag: Option<[u8; 4]>,
) -> Result<GlobalFormId, Error> {
    let id = world.resolve_from(owner, raw).ok_or(Error::Form(owner))?;
    if raw == 0
        || world
            .record(id)
            .is_none_or(|rec| rec.is_deleted() || tag.is_some_and(|tag| rec.tag != Tag(tag)))
    {
        return Err(Error::Form(id));
    }
    Ok(id)
}

fn function_value(
    world: &mut World,
    vm: Option<&Vm>,
    context: Context,
    subject: GlobalFormId,
    c: &Condition,
) -> Result<f32, Error> {
    let raw = c.parameters[0];
    Ok(match c.function {
        6 | 8 => {
            no_second_parameter(c)?;
            let axis = usize::try_from(raw).map_err(|_| Error::Parameter(c.function, raw))?;
            if axis >= 3 {
                return Err(Error::Parameter(c.function, raw));
            }
            let reference = world.reference(subject).ok_or(Error::Form(subject))?;
            if reference.deleted {
                return Err(Error::Form(subject));
            }
            if c.function == 6 {
                reference.position[axis]
            } else {
                reference.rotation[axis].to_degrees()
            }
        }
        10 | 11 => {
            no_second_parameter(c)?;
            let axis = usize::try_from(raw).map_err(|_| Error::Parameter(c.function, raw))?;
            if axis >= 3 {
                return Err(Error::Parameter(c.function, raw));
            }
            let record = world.record(subject).ok_or(Error::Form(subject))?;
            let values = match record.tag.as_bytes() {
                b"REFR" => {
                    let transform = Refr::from_record(record)
                        .ok()
                        .and_then(|reference| reference.transform)
                        .ok_or(Error::Form(subject))?;
                    if c.function == 10 {
                        transform.position
                    } else {
                        transform.rotation.map(f32::to_degrees)
                    }
                }
                b"ACHR" => {
                    let data = record
                        .find(Tag(*b"DATA"))
                        .filter(|data| data.len() >= 24)
                        .ok_or(Error::Form(subject))?;
                    let offset = if c.function == 10 { 0 } else { 12 };
                    let mut values = [0.0; 3];
                    for (index, value) in values.iter_mut().enumerate() {
                        let start = offset + index * 4;
                        let bytes = data[start..start + 4]
                            .try_into()
                            .map_err(|_| Error::Form(subject))?;
                        *value = f32::from_le_bytes(bytes);
                    }
                    if c.function == 11 {
                        values = values.map(f32::to_degrees);
                    }
                    values
                }
                _ => return Err(Error::Form(subject)),
            };
            values[axis]
        }
        25 | 26 | 81 | 219 | 313 | 318 => {
            no_parameter(c)?;
            let reference = world.reference(subject).ok_or(Error::Form(subject))?;
            if reference.deleted {
                return Err(Error::Form(subject));
            }
            return Err(Error::Unavailable(c.function));
        }
        49 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            if actor.ai.sleeping {
                3.0
            } else if actor.ai.procedure == crate::ai::Procedure::Sleep {
                2.0
            } else {
                0.0
            }
        }
        91 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.alerted)
        }
        98 | 106 | 107 | 110 | 127 => {
            if c.function != 98 {
                no_parameter(c)?;
            }
            world.actor(subject).ok_or(Error::Form(subject))?;
            return Err(Error::Unavailable(c.function));
        }
        111 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(matches!(
                actor.ai.procedure,
                crate::ai::Procedure::Wait { .. }
            ))
        }
        125 => {
            no_parameter(c)?;
            let guard_faction = world
                .resolve_from(subject, 0x0008_6eee)
                .ok_or(Error::Form(subject))?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.factions.contains_key(&guard_faction))
        }
        185 | 286 | 287 | 327 | 339 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            match c.function {
                185 => boolean(actor.swimming),
                286 => boolean(actor.sneaking),
                287 => boolean(actor.running),
                327 => boolean(actor.riding_mount),
                339 => boolean(world.last_ridden_horse == Some(subject)),
                _ => unreachable!(),
            }
        }
        14 | 128 | 277 | 408 | 430 | 494 => {
            actor_query(world, context.owner, subject, c.function, raw)?
        }
        1 => {
            let other = resolve_reference(world, context.owner, raw)?;
            world.distance(subject, other)
        }
        18 => world.calendar().hour as f32,
        46 => boolean(world.actor(subject).is_some_and(|actor| actor.dead)),
        67 => {
            let cell = resolve(world, context.owner, raw, Some(*b"CELL"))?;
            boolean(
                world
                    .reference(subject)
                    .and_then(|reference| reference.cell)
                    == Some(cell),
            )
        }
        230 => {
            let cell = resolve(world, context.owner, raw, Some(*b"CELL"))?;
            let reference = resolve_reference(world, context.owner, c.parameters[1])?;
            let parameter_cell = world
                .reference(reference)
                .ok_or(Error::Form(reference))?
                .cell;
            boolean(parameter_cell == Some(cell))
        }
        250 => {
            no_second_parameter(c)?;
            let location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            boolean(
                world
                    .location(location)
                    .ok_or(Error::Form(location))?
                    .cleared,
            )
        }
        280 => {
            let cell = resolve(world, context.owner, raw, Some(*b"CELL"))?;
            let owner = if c.parameters[1] == 0 {
                None
            } else {
                let owner = resolve(world, context.owner, c.parameters[1], None)?;
                if ![Some(Tag(*b"NPC_")), Some(Tag(*b"FACT"))].contains(&world.tag_of(owner)) {
                    return Err(Error::Form(owner));
                }
                Some(owner)
            };
            boolean(world.cell_state(cell).ok_or(Error::Form(cell))?.owner == owner)
        }
        310 => {
            no_second_parameter(c)?;
            let wanted = resolve(world, context.owner, raw, Some(*b"WRLD"))?;
            boolean(reference_worldspace(world, subject) == Some(wanted))
        }
        325 => {
            no_second_parameter(c)?;
            package_location_condition(world, subject, c, raw)?
        }
        445 => {
            no_second_parameter(c)?;
            let zone = resolve(world, context.owner, raw, Some(*b"ECZN"))?;
            let subject_zone = world
                .encounter_zone(subject)
                .map_err(|_| Error::Form(subject))?;
            boolean(subject_zone == Some(zone))
        }
        446 => {
            no_second_parameter(c)?;
            if raw > 2 {
                return Err(Error::Parameter(c.function, raw));
            }
            return Err(Error::Unavailable(c.function));
        }
        491 => {
            no_parameter(c)?;
            boolean(world.reference(subject).ok_or(Error::Form(subject))?.on_map)
        }
        519 => {
            no_parameter(c)?;
            return Err(Error::Unavailable(c.function));
        }
        561 => {
            no_second_parameter(c)?;
            let ref_type = resolve(world, context.owner, raw, Some(*b"LCRT"))?;
            let record = world.record(subject).ok_or(Error::Form(subject))?;
            let matches = record
                .all(Tag(*b"XLRT"))
                .filter(|bytes| bytes.len() == 4)
                .filter_map(|bytes| <[u8; 4]>::try_from(bytes).ok().map(u32::from_le_bytes))
                .any(|raw| world.resolve_from(subject, raw) == Some(ref_type));
            boolean(matches)
        }
        562 => {
            no_second_parameter(c)?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            boolean(world.keywords(location).contains(&keyword))
        }
        563 => {
            no_second_parameter(c)?;
            let ref_type = resolve(world, context.owner, raw, Some(*b"LCRT"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            let record = world.record(location).ok_or(Error::Form(location))?;
            let matches = record
                .all(Tag(*b"ACSR"))
                .chain(record.all(Tag(*b"LCSR")))
                .filter(|bytes| bytes.len() >= 4)
                .filter_map(|bytes| {
                    <[u8; 4]>::try_from(&bytes[..4])
                        .ok()
                        .map(u32::from_le_bytes)
                })
                .any(|raw| world.resolve_from(location, raw) == Some(ref_type));
            boolean(matches)
        }
        565 => {
            no_second_parameter(c)?;
            let location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            boolean(world.editor_location(subject) == Some(location))
        }
        567 => {
            no_second_parameter(c)?;
            if !world.alias_is_location(context.quest.ok_or(Error::Target(5))?, raw) {
                return Err(Error::Target(5));
            }
            let location = world
                .alias_ref(context.quest.ok_or(Error::Target(5))?, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            boolean(world.editor_location(subject) == Some(location))
        }
        569 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.blocking)
        }
        591 | 592 => {
            let location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            let ref_type = resolve(world, context.owner, c.parameters[1], Some(*b"LCRT"))?;
            location_ref_type_count(world, location, ref_type, c.function == 591)?
        }
        598 => {
            no_second_parameter(c)?;
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let location = world
                .alias_ref(quest, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            boolean(
                world
                    .location(location)
                    .ok_or(Error::Form(location))?
                    .cleared,
            )
        }
        600 | 601 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let location = world
                .alias_ref(quest, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            let ref_type = resolve(world, context.owner, c.parameters[1], Some(*b"LCRT"))?;
            location_ref_type_count(world, location, ref_type, c.function == 600)?
        }
        603 => {
            let other = resolve_reference(world, context.owner, raw)?;
            let keyword = resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            let other_location = world
                .current_location(other)
                .ok_or(Error::LocationContext(other))?;
            boolean(location == other_location && world.keywords(location).contains(&keyword))
        }
        604 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            if world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let other = world
                .alias_ref(quest, raw)
                .filter(|other| {
                    let tag = world.tag_of(*other);
                    tag == Some(Tag(*b"ACHR")) || tag == Some(Tag(*b"REFR"))
                })
                .ok_or(Error::Target(5))?;
            let keyword = resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            let other_location = world
                .current_location(other)
                .ok_or(Error::LocationContext(other))?;
            boolean(location == other_location && world.keywords(location).contains(&keyword))
        }
        605 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let alias_location = world
                .alias_ref(quest, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            let location = resolve(world, context.owner, c.parameters[1], Some(*b"LCTN"))?;
            boolean(alias_location == location)
        }
        610 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let location = world
                .alias_ref(quest, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            let keyword = resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?;
            boolean(world.keywords(location).contains(&keyword))
        }
        625 => {
            no_second_parameter(c)?;
            let _location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            return Err(Error::Unavailable(c.function));
        }
        626 => {
            no_second_parameter(c)?;
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let _location = world
                .alias_ref(quest, raw)
                .filter(|location| world.tag_of(*location) == Some(Tag(*b"LCTN")))
                .ok_or(Error::Target(5))?;
            return Err(Error::Unavailable(c.function));
        }
        651 => {
            no_second_parameter(c)?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            world
                .location(location)
                .and_then(|state| state.keyword_data.get(&keyword).copied())
                .unwrap_or(0.0)
        }
        718 => {
            no_parameter(c)?;
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            if world.tag_of(base) != Some(Tag(*b"DOOR")) {
                return Err(Error::Form(subject));
            }
            boolean(!world.reference(subject).ok_or(Error::Form(subject))?.locked)
        }
        69 => {
            let race = resolve(world, context.owner, raw, Some(*b"RACE"))?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.race == Some(race)),
            )
        }
        70 => {
            if raw > 1 {
                return Err(Error::Parameter(c.function, raw));
            }
            let (_, npc) = world
                .actor_npc_part(subject, crate::npc::template::TRAITS)
                .map_err(|_| Error::Form(subject))?;
            boolean(npc.is_female() == (raw == 1))
        }
        71 | 73 => {
            let faction = resolve(world, context.owner, raw, Some(*b"FACT"))?;
            let rank = world
                .actor(subject)
                .and_then(|actor| actor.factions.get(&faction).copied())
                .unwrap_or(-1);
            if c.function == 71 {
                boolean(rank >= 0)
            } else {
                rank as f32
            }
        }
        80 => world.actor(subject).map_or(0.0, |actor| actor.level as f32),
        116 => {
            // IsIntimidatedbyPlayer takes no parameter (wbDefinitionsTES5.pas:286,
            // Raw_Function_Data.wikitext 0x1074). CK ties it to SetIntimidated
            // (ck-wiki/IsIntimidated_-_Actor.wikitext,
            // ck-wiki/SetIntimidated_-_Actor.wikitext), a persistent per-actor
            // flag. No setter exists yet, so this reads the LastBribedIntimidated
            // actor value that backs both the bribe and intimidate flags: a
            // nonzero value means an attempt against this actor already landed.
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .values
                    .get("lastbribedintimidated")
                    .current()
                    != 0.0,
            )
        }
        225 | 315 => {
            // GetPersuasionNumber and GetTotalPersuasionNumber take no parameter
            // either (wbDefinitionsTES5.pas:340,385, Raw_Function_Data.wikitext
            // 0x10E1, 0x113B). Neither source documents the returned encoding, so
            // both read the subject's current Speechcraft and let the INFO
            // condition branch on the skill value, matching the done check and
            // docs/game.md. Current is base plus permanent plus temporary plus
            // damage, so an active perk or effect bonus is felt. Current already
            // holds the permanent component, adding it again would count a perk
            // twice. No source separates the two functions, so they agree.
            no_parameter(c)?;
            world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .values
                .get("speechcraft")
                .current()
        }
        403 => {
            let other = resolve_reference(world, context.owner, raw)?;
            world
                .actor(subject)
                .and_then(|actor| actor.relationships.get(&other).copied())
                .unwrap_or(0) as f32
        }
        566 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            boolean(world.alias_ref(quest, raw) == Some(subject))
        }
        47 => {
            let item = resolve(world, context.owner, raw, None)?;
            inventory_count(world, subject, item, &mut Vec::new())? as f32
        }
        56 | 58 | 59 | 543 => {
            let quest = resolve(world, context.owner, raw, Some(*b"QUST"))?;
            let state = world.quest_state(quest).ok_or(Error::Form(quest))?;
            match c.function {
                56 => boolean(state.running),
                58 => f32::from(state.highest),
                59 => boolean(
                    u16::try_from(c.parameters[1]).is_ok_and(|stage| state.done.contains(&stage)),
                ),
                _ => boolean(state.completed),
            }
        }
        53 | 79 => {
            let name = c.strings[1]
                .as_deref()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| Error::Variable("missing CIS2".into()))?;
            let form = if c.function == 53 {
                resolve_reference(world, context.owner, raw)?
            } else {
                resolve(world, context.owner, raw, Some(*b"QUST"))?
            };
            let values = world
                .object_id(Handle::Form(form))
                .map(|object| {
                    vm.map(|vm| vm.conditional_vars(object, name))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            match values.as_slice() {
                [Value::Int(value)] => *value as f32,
                [Value::Float(value)] => *value,
                [Value::Bool(value)] => boolean(*value),
                _ => return Err(Error::Variable(name.into())),
            }
        }
        470 => {
            no_parameter(c)?;
            world
                .reference(subject)
                .ok_or(Error::Form(subject))?
                .destruction_stage as f32
        }
        528 => {
            if raw > 4 || c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, raw.max(c.parameters[1])));
            }
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .critical_stage
                    == raw as i32,
            )
        }
        390 | 576 | 646 | 647 => return Err(Error::Function(c.function)),
        72 => {
            let base = resolve(world, context.owner, raw, None)?;
            boolean(world.base_of(subject) == Some(base))
        }
        74 => {
            let global = resolve(world, context.owner, raw, Some(*b"GLOB"))?;
            world.global_value(global)
        }
        136 => boolean(subject == resolve_reference(world, context.owner, raw)?),
        182 => {
            let item = resolve(world, context.owner, raw, None)?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.equipped.contains(&item)),
            )
        }
        264 => {
            let spell = resolve(world, context.owner, raw, Some(*b"SPEL"))?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.spells.contains(&spell)),
            )
        }
        289 => boolean(world.actor(subject).is_some_and(|actor| actor.in_combat)),
        359 => {
            let wanted = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            boolean(world.current_location(subject).is_some_and(|current| {
                current == wanted || world.location_is_child(wanted, current)
            }))
        }
        360 => {
            no_second_parameter(c)?;
            let quest = context.quest.ok_or(Error::Target(5))?;
            if !world.alias_is_location(quest, raw) {
                return Err(Error::Target(5));
            }
            let location = world.alias_ref(quest, raw).ok_or(Error::Target(5))?;
            if world.tag_of(location) != Some(Tag(*b"LCTN")) {
                return Err(Error::Form(location));
            }
            boolean(world.current_location(subject).is_some_and(|current| {
                current == location || world.location_is_child(location, current)
            }))
        }
        448 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let perk = resolve(world, context.owner, raw, Some(*b"PERK"))?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.perks.contains(&perk)),
            )
        }
        560 => {
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            boolean(world.has_keyword(subject, keyword))
        }
        606 => {
            let location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            let keyword = resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?;
            world
                .location(location)
                .and_then(|state| state.keyword_data.get(&keyword).copied())
                .unwrap_or(0.0)
        }
        444 => {
            no_second_parameter(c)?;
            let list = resolve(world, context.owner, raw, Some(*b"FLST"))?;
            let locations = world.form_list(list).ok_or(Error::Form(list))?.clone();
            boolean(world.current_location(subject).is_some_and(|current| {
                locations.iter().any(|location| {
                    *location == current || world.location_is_child(*location, current)
                })
            }))
        }
        629 | 630 => {
            let form = if c.function == 629 {
                resolve(world, context.owner, raw, Some(*b"QUST"))?
            } else {
                resolve_reference(world, context.owner, raw)?
            };
            let name = c.strings[1]
                .as_deref()
                .ok_or_else(|| Error::Variable("missing CIS2".into()))?;
            let values = world
                .object_id(Handle::Form(form))
                .map(|object| {
                    vm.map(|vm| vm.conditional_vars(object, name))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            match values.as_slice() {
                [Value::Int(value)] => *value as f32,
                [Value::Float(value)] => *value,
                [Value::Bool(value)] => boolean(*value),
                _ => return Err(Error::Variable(name.into())),
            }
        }
        655 => {
            // GetIntimidateSuccess takes no parameter either (wbDefinitionsTES5.pas:562,
            // Raw_Function_Data.wikitext 0x128F). Its wiki page for the matching
            // native, WillIntimidateSucceed, points at the "Mod Player Intimidation"
            // perk entry point for the real formula, which this engine does not run
            // yet (the native is still a stub in natives.rs). This approximates the
            // same shape without the entry point: the player's Speechcraft against
            // the subject's Confidence (AIDT 0 Cowardly to 4 Foolhardy), scaled onto
            // the 0 to 100 skill range.
            no_parameter(c)?;
            let player = world.player();
            let speechcraft = world
                .actor(player)
                .ok_or(Error::Form(player))?
                .values
                .get("speechcraft")
                .current();
            let subject_actor = world.actor(subject).ok_or(Error::Form(subject))?;
            if subject_actor.values.contains("confidence") {
                let confidence = subject_actor.values.get("confidence").current();
                boolean(speechcraft >= confidence * 25.0)
            } else {
                // No AIDT subrecord on the speaker's NPC_ record: the
                // source has made no claim about Confidence at all, so
                // reading the unset default (0.0, Cowardly) would treat a
                // record silent on the point as the easiest possible
                // target to intimidate. Fail closed instead (see
                // dlg_07_missing_aidt_adversarial.rs).
                boolean(false)
            }
        }
        659 => boolean(context.tempering_enchanted.ok_or(Error::TemperingContext)?),
        other => return Err(Error::Function(other)),
    })
}

/// Shared actor queries keep package and gameplay predicates on the same state.
pub(crate) fn actor_query(
    world: &mut World,
    owner: GlobalFormId,
    subject: GlobalFormId,
    function: u16,
    parameter: u32,
) -> Result<f32, Error> {
    if world
        .reference(subject)
        .is_none_or(|reference| reference.deleted)
    {
        return Err(Error::Form(subject));
    }
    if function == 408 {
        let other = resolve_reference(world, owner, parameter)?;
        if world.actor(other).is_none() {
            return Err(Error::Form(other));
        }
        let actor = world.actor(subject).ok_or(Error::Form(subject))?;
        return Ok(boolean(actor.dead && actor.killer == Some(other)));
    }
    let name = match function {
        128 => "stamina",
        430 => "health",
        14 | 277 | 494 => actor_values::name(parameter).ok_or(Error::ActorValue(parameter))?,
        _ => return Err(Error::Function(function)),
    };
    let actor = world.actor(subject).ok_or(Error::Form(subject))?;
    let components = actor.values.get(name);
    let value = match function {
        14 => components.current(),
        277 => components.base(),
        494 => (f64::from(components.base()) + f64::from(components.permanent())) as f32,
        _ => actor.values.percentage(name),
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFinite)
    }
}

/// GetItemCount totals a base, FormList or keyword, and selects one item reference.
///
/// FormList recursion is bounded so a modded cycle cannot make a native call diverge.
pub(crate) fn inventory_count(
    world: &mut World,
    subject: GlobalFormId,
    item: GlobalFormId,
    visiting: &mut Vec<GlobalFormId>,
) -> Result<i32, Error> {
    if visiting.len() >= 64 || visiting.contains(&item) {
        return Err(Error::Inventory);
    }
    if let Some(list) = world.form_list(item).cloned() {
        visiting.push(item);
        let mut total = 0i32;
        for entry in list {
            total = total
                .checked_add(inventory_count(world, subject, entry, visiting)?)
                .ok_or(Error::Inventory)?;
        }
        visiting.pop();
        return Ok(total);
    }
    if world.is_reference(item) {
        let base = world.base_of(item).ok_or(Error::Form(item))?;
        let inventory = &world
            .reference(subject)
            .ok_or(Error::Form(subject))?
            .inventory;
        let maximum = inventory.count(base);
        let total = inventory
            .stacks(base)
            .iter()
            .filter(|stack| {
                stack.extra(32).is_some_and(|extra| {
                    matches!(extra.fields.as_slice(), [Field::Form(Some(id))] if *id == item)
                })
            })
            .fold(0i32, |total, stack| {
                total.saturating_add(stack.count().max(0)).min(maximum)
            });
        return Ok(total);
    }
    if world.tag_of(item) == Some(Tag(*b"KYWD")) {
        let items: Vec<_> = world
            .reference(subject)
            .ok_or(Error::Form(subject))?
            .inventory
            .iter()
            .map(|(id, count)| (*id, count))
            .collect();
        let mut total = 0i32;
        for (id, count) in items {
            if world.has_keyword(id, item) {
                total = total.checked_add(count).ok_or(Error::Inventory)?;
            }
        }
        return Ok(total);
    }
    Ok(world.item_count(subject, item))
}

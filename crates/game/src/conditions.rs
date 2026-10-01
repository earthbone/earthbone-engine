// SPDX-License-Identifier: GPL-3.0-or-later

//! Gameplay CTDA execution. Field/function identities come from xEdit
//! wbConditions/wbConditionFunctions. CK documents the state queries and
//! conditional Papyrus variables. See docs/game.md for the implemented set.

use crate::detection::DetectionLevel;
use crate::package::DataValue;
use crate::{inventory::Field, Handle, World};
use soe_esm::{
    records::gameplay::{Comparison, Condition},
    records::refr::Refr,
    GlobalFormId, Tag,
};
use soe_papyrus::{Value, Vm};
use std::collections::BTreeSet;

mod actor_values;

/// The record supplies local FormID mapping. Subject and target are runtime forms.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub owner: GlobalFormId,
    pub subject: GlobalFormId,
    pub target: Option<GlobalFormId>,
    pub quest: Option<GlobalFormId>,
    pub tempering_enchanted: Option<bool>,
    pub tempering_item: Option<GlobalFormId>,
    /// Poison selection for an alchemy perk entry point.
    pub alchemy_making_poison: Option<bool>,
    /// Effect currently tested by an alchemy perk entry point.
    pub alchemy_effect: Option<GlobalFormId>,
    /// Magic item whose spell or effect conditions are currently evaluated.
    pub magic_item: Option<GlobalFormId>,
    /// Dual-cast state is supplied by a cast driver when it is available.
    pub dual_cast: Option<bool>,
    /// The skill-use event whose PERK conditions are being evaluated.
    pub skill_use: Option<SkillUseContext>,
    /// Light level supplied by the host for the subject actor.
    pub light_level: Option<f32>,
}

/// Skill-use information visible to EPModSkillUsage PERK conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillUseContext {
    pub actor: GlobalFormId,
    pub action: u32,
    pub advance_object: Option<GlobalFormId>,
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
    #[error("condition function {0} needs runtime state that is not available")]
    Unavailable(u16),
    #[error("condition item health extra is malformed or non-finite")]
    ItemHealth,
    #[error("condition function {0} has no usable runtime contract in the local sources")]
    Contract(u16),
    #[error("condition function {0} requires alchemy entry-point context")]
    AlchemyContext(u16),
    #[error("condition function {0} requires magic context")]
    MagicContext(u16),
    #[error("condition function {0} has no runtime state contract for this form")]
    StateContract(u16),
    #[error("condition function {0} requires matching skill-use event context")]
    SkillUseContext(u16),
    #[error("condition function {0} requires a light level from 0 to 150")]
    LightLevelContext(u16),
    #[error("condition function {0} has no usable CK contract")]
    UnavailableContract(u16),
    #[error("condition requires the subject's current package {0}")]
    PackageContext(GlobalFormId),
    #[error("condition requires the subject's current location {0}")]
    LocationContext(GlobalFormId),
    #[error("condition function {function} has no usable runtime contract: {reason}")]
    UnavailableState { function: u16, reason: &'static str },
    #[error("condition function {0} has no usable runtime contract: {1}")]
    UnavailableContractReason(u16, &'static str),
    #[error("condition function {0} requires unavailable runtime context")]
    Context(u16),
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
            | 123
            | 434
            | 435
            | 731
            | 128
            | 277
            | 408
            | 550
            | 430
            | 635
            | 494
            | 476
            | 493
            | 18
            | 46
            | 47
            | 56
            | 58
            | 59
            | 248
            | 249
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
            | 84
            | 136
            | 182
            | 264
            | 289
            | 323
            | 359
            | 338
            | 448
            | 543
            | 560
            | 571
            | 579
            | 595
            | 596
            | 602
            | 606
            | 627
            | 632
            | 629
            | 630
            | 639
            | 655
            | 659
            | 254
            | 258
            | 259
            | 261
            | 365
            | 410
            | 683
            | 698
            | 704
            | 675
            | 636
            | 660
            | 723
            | 214
            | 223
            | 378
            | 500
            | 501
            | 570
            | 693
            | 696
            | 699
            | 706
            | 713
            | 721
            | 724
            | 447
            | 568
            | 640
            | 691
            | 692
            | 711
            | 734
            | 735
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
            | 513
            | 580
            | 694
            | 707
            | 710
            | 676
            | 99
            | 142
            | 159
            | 162
            | 163
            | 175
            | 24
            | 35
            | 45
            | 61
            | 77
            | 372
            | 391
            | 414
            | 415
            | 416
            | 426
            | 432
            | 463
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

fn first_parameter_only(c: &Condition) -> Result<(), Error> {
    no_second_parameter(c)
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
        24 => {
            no_parameter(c)?;
            let actor_scale = if world.actor(subject).is_some() {
                let base = world.base_of(subject).ok_or(Error::Form(subject))?;
                world
                    .actor_controller_scale(base)
                    .map_err(|_| Error::Form(subject))?
            } else {
                1.0
            };
            let reference = world.reference(subject).ok_or(Error::Form(subject))?;
            if reference.deleted || !reference.scale.is_finite() {
                return Err(Error::Form(subject));
            }
            reference.scale * actor_scale
        }
        27 | 36 | 39 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "the local CK corpus does not define a runtime contract",
            ));
        }
        35 => {
            no_parameter(c)?;
            let reference = world.reference(subject).ok_or(Error::Form(subject))?;
            if reference.deleted {
                return Err(Error::Form(subject));
            }
            boolean(!reference.enabled)
        }
        45 => {
            let observer = resolve_reference(world, context.owner, raw)?;
            if world.actor(subject).is_none() || world.actor(observer).is_none() {
                return Err(Error::Form(if world.actor(subject).is_none() {
                    subject
                } else {
                    observer
                }));
            }
            boolean(
                world
                    .awareness(observer, subject)
                    .is_some_and(|awareness| awareness.level == DetectionLevel::Combat),
            )
        }
        61 => {
            no_parameter(c)?;
            boolean(world.actor(subject).ok_or(Error::Form(subject))?.alerted)
        }
        77 => {
            no_parameter(c)?;
            world.random_int(0, 99) as f32
        }
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
        640 => {
            no_parameter_second(c)?;
            actor_query(world, context.owner, subject, c.function, raw)?
        }
        691 | 692 => {
            let event = context
                .skill_use
                .filter(|event| event.actor == subject && world.actor(event.actor).is_some())
                .ok_or(Error::SkillUseContext(c.function))?;
            match c.function {
                691 => {
                    let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
                    no_parameter_second(c)?;
                    boolean(
                        event
                            .advance_object
                            .is_some_and(|object| world.has_keyword(object, keyword)),
                    )
                }
                _ => {
                    no_parameter_second(c)?;
                    if !(1..=14).contains(&raw) {
                        return Err(Error::Parameter(c.function, raw));
                    }
                    boolean(event.action == raw)
                }
            }
        }
        711 => {
            no_parameter(c)?;
            world.actor(subject).ok_or(Error::Form(subject))?;
            context
                .light_level
                .filter(|value| value.is_finite() && (0.0..=150.0).contains(value))
                .ok_or(Error::LightLevelContext(c.function))?
        }
        734 => {
            no_parameter(c)?;
            let values = &world.actor(subject).ok_or(Error::Form(subject))?.values;
            boolean(values.get("inventoryweight").current() > values.get("carryweight").current())
        }
        735 => return Err(Error::UnavailableContract(c.function)),
        1 => {
            let other = resolve_reference(world, context.owner, raw)?;
            world.distance(subject, other)
        }
        99 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let other = resolve_reference(world, context.owner, raw)?;
            let (position, heading) = world
                .reference(subject)
                .map(|reference| (reference.position, reference.rotation[2]))
                .ok_or(Error::Form(subject))?;
            let target = world
                .reference(other)
                .map(|reference| reference.position)
                .ok_or(Error::Form(other))?;
            let dx = target[0] - position[0];
            let dy = target[1] - position[1];
            let bearing = dy.atan2(dx).to_degrees();
            let heading = heading.to_degrees();
            (heading - bearing + 180.0).rem_euclid(360.0) - 180.0
        }
        142 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            crate::ai::movement_speed(world, subject, crate::package::speed::WALK)
        }
        476 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.actor_base(base).ok_or(Error::Form(base))?.protected)
        }
        159 => {
            no_parameter(c)?;
            boolean(furniture_marker_type(world, subject, 1)?)
        }
        162 | 163 => {
            let furniture = if c.function == 162 {
                resolve_reference(world, context.owner, raw)?
            } else {
                resolve(world, context.owner, raw, Some(*b"FURN"))?
            };
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let Some((current, _)) = world.furniture_seat(subject) else {
                if world.actor(subject).is_none() {
                    return Err(Error::Form(subject));
                }
                return Ok(0.0);
            };
            if c.function == 162 {
                boolean(current == furniture)
            } else {
                boolean(world.reference_ref(current).and_then(|r| r.base) == Some(furniture))
            }
        }
        175 => {
            no_parameter(c)?;
            let player = world.player();
            boolean(furniture_marker_type(world, player, 2)?)
        }
        493 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let form = resolve(world, context.owner, raw, None)?;
            if world.tag_of(form) == Some(Tag(*b"MGEF")) {
                boolean(false)
            } else if world.tag_of(form) != Some(Tag(*b"SPEL")) {
                return Err(Error::StateContract(c.function));
            } else {
                let player = world.player();
                boolean(
                    world
                        .actor(player)
                        .ok_or(Error::Form(player))?
                        .spells
                        .contains(&form),
                )
            }
        }
        18 => world.calendar().hour as f32,
        46 => boolean(world.actor(subject).is_some_and(|actor| actor.dead)),
        84 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let base = resolve(world, context.owner, raw, Some(*b"NPC_"))?;
            world.actor_base(base).ok_or(Error::Form(base))?.dead_count as f32
        }
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
        123 => {
            // IsGreetingPlayer (wbDefinitionsTES5.pas:290) takes no parameters.
            // Its subject is true during automatic ForceGreet selection.
            no_parameter(c)?;
            boolean(
                world.actor(subject).is_some() && world.dialogue.greeting_actor == Some(subject),
            )
        }
        434 | 435 => {
            no_parameter(c)?;
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let Some(session) = world
                .dialogue_session()
                .filter(|session| session.speaker == subject)
            else {
                return Ok(0.0);
            };
            let Some(info) = session.current_info else {
                return Ok(0.0);
            };
            let (_, data) = world.response_data(info).map_err(|_| Error::Form(info))?;
            match data.responses.get(session.response_index as usize) {
                Some(response) if c.function == 434 => response.emotion as f32,
                Some(response) => response.emotion_value as f32,
                None => 0.0,
            }
        }
        550 => {
            let scene = resolve(world, context.owner, raw, Some(*b"SCEN"))?;
            let slot = c.parameters[1];
            let Some(completed) = world.scene_action_complete(scene, slot) else {
                return Err(Error::Parameter(c.function, slot));
            };
            boolean(completed)
        }
        635 => {
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .doing_favor,
            )
        }
        731 => {
            // CanFlyHere reads the current worldspace's max-height data.
            no_parameter(c)?;
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let cell = world
                .reference(subject)
                .and_then(|reference| reference.cell)
                .ok_or(Error::Form(subject))?;
            boolean(world.cell_has_worldspace_max_height_data(cell))
        }
        225 | 315 => {
            // DLG-07 established current Speechcraft as the condition value.
            no_parameter(c)?;
            world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .values
                .get("speechcraft")
                .current()
        }
        248 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let scene = resolve(world, context.owner, raw, Some(*b"SCEN"))?;
            boolean(world.scenes.contains(&scene))
        }
        249 => {
            no_parameter(c)?;
            let is_actor = world.actor(subject).is_some();
            let player = world.player();
            boolean(
                is_actor
                    && world.dialogue.session.as_ref().is_some_and(|session| {
                        (session.speaker == subject && session.listener == player)
                            || (session.listener == subject && session.speaker == player)
                    }),
            )
        }
        403 => {
            let other = resolve_reference(world, context.owner, raw)?;
            world
                .actor(subject)
                .and_then(|actor| actor.relationships.get(&other).copied())
                .unwrap_or(0) as f32
        }
        513 => {
            // xEdit identifies a placed NPC parameter. The CK dump has no
            // condition page, so match the runtime combat target relation
            // exposed by Actor.GetCombatTarget: the subject targets the
            // parameter actor.
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let target = resolve_reference(world, context.owner, raw)?;
            if world.actor(target).is_none() {
                return Err(Error::Form(target));
            }
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .combat_target
                    == Some(target),
            )
        }
        702 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "the runtime tracks stagger state, but no recoil state",
            ));
        }
        705 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "CK does not define the actor-to-actor help decision",
            ));
        }
        707 => {
            no_parameter_second(c)?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let target = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .combat_target;
            let Some(target) = target else {
                return Ok(0.0);
            };
            if world.actor(target).is_none() {
                return Err(Error::Form(target));
            }
            boolean(world.has_keyword(target, keyword))
        }
        709 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "the runtime does not model combat group membership",
            ));
        }
        710 => {
            no_parameter(c)?;
            let package = world.actor(subject).ok_or(Error::Form(subject))?.ai.package;
            boolean(package.is_some_and(|id| {
                crate::ai::package(world, id)
                    .is_some_and(|pack| pack.has_flag(crate::package::flags::IGNORE_COMBAT))
            }))
        }
        719 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "CK names hostility but does not define its faction and crime rules",
            ));
        }
        254 | 365 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            let value = if c.function == 254 {
                world.race_is_playable(base)
            } else {
                world.race_is_child(base)
            }
            .ok_or(Error::Form(base))?;
            boolean(value)
        }
        258 => {
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let other = resolve_reference(world, context.owner, raw)?;
            if world.actor(other).is_none() {
                return Err(Error::Form(other));
            }
            let association = resolve(world, context.owner, c.parameters[1], Some(*b"ASTP"))?;
            let first = world.base_of(subject).ok_or(Error::Form(subject))?;
            let second = world.base_of(other).ok_or(Error::Form(other))?;
            boolean(world.actors_have_association(first, second, association))
        }
        259 | 261 => {
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let other = if c.function == 259 && raw == 0 {
                None
            } else {
                let other = resolve_reference(world, context.owner, raw)?;
                if world.actor(other).is_none() {
                    return Err(Error::Form(other));
                }
                Some(other)
            };
            let first = world.base_of(subject).ok_or(Error::Form(subject))?;
            let matched = if c.function == 259 {
                let second = other
                    .map(|other| world.base_of(other).ok_or(Error::Form(other)))
                    .transpose()?;
                world.actors_have_family_relationship(first, second)
            } else {
                let other = other.ok_or(Error::Form(subject))?;
                let second = world.base_of(other).ok_or(Error::Form(other))?;
                world.actors_have_parent_relationship(first, second)
            };
            boolean(matched)
        }
        410 => {
            let first = resolve(world, context.owner, raw, Some(*b"FACT"))?;
            let second = resolve(world, context.owner, c.parameters[1], Some(*b"FACT"))?;
            world
                .faction(first)
                .ok_or(Error::Form(first))?
                .reactions
                .get(&second)
                .copied()
                .unwrap_or(crate::world::reaction::NEUTRAL) as f32
        }
        566 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            boolean(world.alias_ref(quest, raw) == Some(subject))
        }
        571 => {
            no_second_parameter(c)?;
            let spell = casting_spell(world, subject, c.function, raw)?;
            let current = world.concentration_spell(subject);
            match spell.filter(|spell| Some(*spell) == current) {
                Some(spell) => {
                    world
                        .spell(spell)
                        .map_err(|_| Error::Form(spell))?
                        .data
                        .cast_type as f32
                }
                None => 0.0,
            }
        }
        579 => {
            no_second_parameter(c)?;
            let shout = resolve(world, context.owner, raw, Some(*b"SHOU"))?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.equipped_shout == Some(shout))
        }
        595 => {
            let spell = resolve(world, context.owner, raw, Some(*b"SPEL"))?;
            let source = casting_spell(world, subject, c.function, c.parameters[1])?;
            boolean(source == Some(spell) && world.concentration_spell(subject) == Some(spell))
        }
        596 => {
            let source = casting_spell(world, subject, c.function, raw)?;
            let keyword = resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?;
            let has_keyword = if let Some(spell) = source {
                let definition = world.spell(spell).map_err(|_| Error::Form(spell))?;
                definition.effects.iter().any(|effect| {
                    world
                        .resolve_from(spell, effect.base_effect)
                        .is_some_and(|effect| world.has_keyword(effect, keyword))
                })
            } else {
                false
            };
            boolean(has_keyword)
        }
        602 => {
            no_second_parameter(c)?;
            if raw > 2 {
                return Err(Error::Parameter(c.function, raw));
            }
            world.actor(subject).ok_or(Error::Form(subject))?;
            return Err(Error::UnavailableContractReason(
                c.function,
                "ward animation state is not supplied by the host",
            ));
        }
        627 => {
            no_parameter(c)?;
            world.actor(subject).ok_or(Error::Form(subject))?;
            return Err(Error::UnavailableContractReason(
                c.function,
                "dual-cast animation state is not tracked",
            ));
        }
        632 => {
            no_parameter(c)?;
            world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(world.is_concentration_casting(subject))
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
        372 => {
            let list = resolve(world, context.owner, raw, Some(*b"FLST"))?;
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            let members = world.form_list(list).ok_or(Error::Form(list))?;
            boolean(members.contains(&base))
        }
        414 => {
            let reference = world
                .resolve_from(context.owner, raw)
                .ok_or(Error::Form(context.owner))?;
            if raw == 0
                || !matches!(world.tag_of(reference), Some(Tag(tag)) if tag == *b"ACHR" || tag == *b"REFR")
            {
                return Err(Error::Form(reference));
            }
            boolean(
                world
                    .reference(reference)
                    .is_some_and(|state| !state.deleted),
            )
        }
        391 | 415 | 416 => {
            no_parameter(c)?;
            return Err(Error::Context(c.function));
        }
        426 => {
            let voice_type = resolve(world, context.owner, raw, Some(*b"VTYP"))?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let source = world
                .actor_component_source(subject, crate::npc::template::TRAITS)
                .map_err(|_| Error::Form(subject))?;
            let voice = world
                .record(source)
                .and_then(|record| record.find(Tag(*b"VTCK")))
                .and_then(|bytes| {
                    (bytes.len() >= 4)
                        .then(|| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                })
                .and_then(|raw| world.resolve_from(source, raw));
            boolean(voice == Some(voice_type))
        }
        432 => {
            if raw == 0 || i32::try_from(raw).is_err() {
                return Err(Error::Parameter(c.function, raw));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.type_code(base) == raw as i32)
        }
        463 => {
            resolve_reference(world, context.owner, raw)?;
            return Err(Error::Context(c.function));
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
        101 | 552 => return Err(Error::Contract(c.function)),
        214 => {
            no_parameter_tail(c)?;
            let effect = resolve(world, context.owner, raw, Some(*b"MGEF"))?;
            boolean(world.has_magic_effect(subject, effect))
        }
        223 => {
            no_parameter_tail(c)?;
            let item = resolve(world, context.owner, raw, None)?;
            if !matches!(world.tag_of(item), Some(Tag(tag)) if [*b"SPEL", *b"ALCH", *b"ENCH", *b"SHOU", *b"SCRL"].contains(&tag))
            {
                return Err(Error::Form(item));
            }
            boolean(
                world
                    .active_effects()
                    .any(|effect| effect.target == subject && effect.spell == item),
            )
        }
        378 => {
            no_parameter_tail(c)?;
            let shout = resolve(world, context.owner, raw, Some(*b"SHOU"))?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.shouts.contains(&shout)),
            )
        }
        500 => {
            no_parameter(c)?;
            boolean(
                context
                    .alchemy_making_poison
                    .ok_or(Error::AlchemyContext(c.function))?,
            )
        }
        501 => {
            no_parameter_tail(c)?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let effect = context
                .alchemy_effect
                .ok_or(Error::AlchemyContext(c.function))?;
            boolean(world.has_keyword(effect, keyword))
        }
        570 => {
            no_parameter_tail(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            match raw {
                0 => boolean(actor.left_spell.is_some()),
                1 => boolean(actor.right_spell.is_some()),
                2 => boolean(actor.equipped_shout.is_some()),
                _ => return Err(Error::Parameter(c.function, raw)),
            }
        }
        264 => {
            let spell = resolve(world, context.owner, raw, Some(*b"SPEL"))?;
            boolean(world.has_spell(subject, spell))
        }
        289 => boolean(world.actor(subject).is_some_and(|actor| actor.in_combat)),
        323 => {
            // An engaged actor without a current target is still searching.
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            if !actor.in_combat {
                0.0
            } else if actor.combat_target.is_some() {
                1.0
            } else {
                2.0
            }
        }
        338 => {
            no_parameter(c)?;
            boolean(
                world
                    .reference(subject)
                    .ok_or(Error::Form(subject))?
                    .ignore_friendly_hits,
            )
        }
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
        447 => {
            let name = c.strings[0]
                .as_deref()
                .ok_or_else(|| Error::Variable("missing CIS1".into()))?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            actor
                .graph_floats
                .get(&name.to_ascii_lowercase())
                .copied()
                .ok_or_else(|| Error::Variable(name.into()))?
        }
        675 => {
            let name = c.strings[0]
                .as_deref()
                .ok_or_else(|| Error::Variable("missing CIS1".into()))?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            *actor
                .graph_ints
                .get(&name.to_ascii_lowercase())
                .ok_or_else(|| Error::Variable(name.into()))? as f32
        }
        568 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.sprinting)
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
        639 => {
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let other = resolve_reference(world, context.owner, raw)?;
            let maximum_distance = f32::from_bits(c.parameters[1]);
            if !maximum_distance.is_finite() {
                return Err(Error::NonFinite);
            }
            if maximum_distance < 0.0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            boolean(world.distance(subject, other) <= maximum_distance)
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
        580 | 694 => actor_query(world, context.owner, subject, c.function, raw)?,
        672 | 673 | 674 | 678 | 697 | 701 => {
            unavailable_combat_query(world, context.owner, subject, c)?
        }
        676 => {
            no_parameter(c)?;
            world.actor(subject).ok_or(Error::Form(subject))?;
            return Err(Error::UnavailableContractReason(
                c.function,
                "active voice variation is not tracked",
            ));
        }
        636 => {
            no_parameter(c)?;
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let equipped: Vec<_> = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .equipped
                .iter()
                .copied()
                .collect();
            let mut has_two_handed = false;
            for item in equipped {
                if world.tag_of(item) != Some(Tag(*b"WEAP")) {
                    continue;
                }
                if matches!(weapon_animation_type(world, item)?, 5 | 6 | 7 | 9) {
                    has_two_handed = true;
                    break;
                }
            }
            boolean(has_two_handed)
        }
        660 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let item = context.tempering_item.ok_or(Error::TemperingContext)?;
            if world.record(item).is_none_or(|record| record.is_deleted()) {
                return Err(Error::Form(item));
            }
            boolean(world.has_keyword(item, keyword))
        }
        664 => return Err(Error::Function(664)),
        723 => {
            no_parameter(c)?;
            item_health_percent(world, subject)?
        }
        681 => return Err(Error::Function(681)),
        693 => {
            let item = context.magic_item.ok_or(Error::MagicContext(c.function))?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            boolean(world.has_keyword(item, keyword))
        }
        696 => {
            let item = context.magic_item.ok_or(Error::MagicContext(c.function))?;
            if actor_values::name(raw).is_none() {
                return Err(Error::ActorValue(raw));
            }
            let effects = match world.tag_of(item) {
                Some(Tag(tag)) if tag == *b"SPEL" => {
                    world.spell(item).map_err(|_| Error::Form(item))?.effects
                }
                Some(Tag(tag)) if tag == *b"ALCH" => {
                    world
                        .ingestible(item)
                        .map_err(|_| Error::Form(item))?
                        .effects
                }
                _ => return Err(Error::Form(item)),
            };
            let mut matches = false;
            for effect in effects {
                let effect = world
                    .resolve_from(item, effect.base_effect)
                    .ok_or(Error::Form(item))?;
                let definition = world
                    .magic_effect(effect)
                    .map_err(|_| Error::Form(effect))?;
                if i32::try_from(raw).ok() == Some(definition.data.magic_skill) {
                    matches = true;
                    break;
                }
            }
            boolean(matches)
        }
        699 => {
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            boolean(world.has_magic_effect_with_keyword(subject, keyword))
        }
        706 => {
            let hand = match raw {
                0 => crate::combat::Hand::Left,
                1 => crate::combat::Hand::Right,
                2 => return Ok(0.0),
                value => return Err(Error::Parameter(c.function, value)),
            };
            boolean(
                world
                    .has_bound_weapon_equipped(subject, hand)
                    .map_err(|_| Error::Form(subject))?,
            )
        }
        713 => {
            let item = context.magic_item.ok_or(Error::MagicContext(c.function))?;
            let perk = resolve(world, context.owner, raw, Some(*b"PERK"))?;
            let spell = world.spell(item).map_err(|_| Error::Form(item))?;
            let casting_perk = if spell.data.half_cost_perk == 0 {
                None
            } else {
                world.resolve_from(item, spell.data.half_cost_perk)
            };
            boolean(casting_perk == Some(perk))
        }
        721 => {
            no_parameter(c)?;
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            let ingestible = world.ingestible(base).map_err(|_| Error::Form(base))?;
            boolean(ingestible.data.flags & (1 << 17) != 0)
        }
        724 => {
            no_parameter(c)?;
            boolean(context.dual_cast.ok_or(Error::MagicContext(c.function))?)
        }
        683 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            if actor.ai.path.is_empty() || actor.ai.arrived {
                0.0
            } else {
                actor.ai.speed
            }
        }
        698 => {
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .allow_flying,
            )
        }
        704 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(!actor.ai.path.is_empty() && !actor.ai.arrived)
        }
        644 | 684 | 714 | 730 => return Err(Error::Unavailable(c.function)),
        other => return Err(Error::Function(other)),
    })
}

fn weapon_animation_type(world: &World, item: GlobalFormId) -> Result<u8, Error> {
    let mut current = item;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current) || seen.len() > 128 {
            return Err(Error::Form(item));
        }
        let record = world
            .record(current)
            .filter(|record| record.tag == Tag(*b"WEAP"))
            .ok_or(Error::Form(current))?;
        if let Some(animation) = record
            .find(Tag(*b"DNAM"))
            .and_then(|data| data.first())
            .copied()
        {
            return Ok(animation);
        }
        let (template, _) = crate::combat::records::Weapon::template_links(record)
            .map_err(|_| Error::Form(current))?;
        let template = template.ok_or(Error::Form(current))?;
        current = world
            .resolve_from(current, template)
            .ok_or(Error::Form(current))?;
    }
}

fn item_health_percent(world: &mut World, subject: GlobalFormId) -> Result<f32, Error> {
    let reference = world
        .reference(subject)
        .filter(|reference| !reference.deleted)
        .ok_or(Error::Form(subject))?;
    let Some(extra) = reference.item.as_ref().and_then(|stack| stack.extra(37)) else {
        return Ok(1.0);
    };
    let [Field::FloatBits(bits)] = extra.fields.as_slice() else {
        return Err(Error::ItemHealth);
    };
    let quality = f32::from_bits(*bits);
    if quality.is_finite() {
        Ok(quality)
    } else {
        Err(Error::ItemHealth)
    }
}

/// These predicates consume parameter 1 and reject any value in parameter 2.
fn no_parameter_tail(c: &Condition) -> Result<(), Error> {
    if c.parameters[1] == 0 {
        Ok(())
    } else {
        Err(Error::Parameter(c.function, c.parameters[1]))
    }
}

/// Resolve an equipped casting source. Voice and instant sources have no
/// spell selection in the current runtime model.
fn casting_spell(
    world: &mut World,
    subject: GlobalFormId,
    function: u16,
    source: u32,
) -> Result<Option<GlobalFormId>, Error> {
    world.actor(subject).ok_or(Error::Form(subject))?;
    match source {
        0 => Ok(world.hand_spell(subject, crate::combat::Hand::Left)),
        1 => Ok(world.hand_spell(subject, crate::combat::Hand::Right)),
        2 | 3 => Ok(None),
        _ => Err(Error::Parameter(function, source)),
    }
}

fn furniture_marker_type(
    world: &mut World,
    actor: GlobalFormId,
    marker_type: u16,
) -> Result<bool, Error> {
    let Some((furniture, marker)) = world.furniture_seat(actor) else {
        if world.actor(actor).is_none() {
            return Err(Error::Form(actor));
        }
        return Ok(false);
    };
    let reference = world.reference(furniture).ok_or(Error::Form(furniture))?;
    let base = reference.base.ok_or(Error::Form(furniture))?;
    let record = world.record(base).ok_or(Error::Form(base))?;
    if record.tag != Tag(*b"FURN") {
        return Err(Error::Form(base));
    }
    Ok(record
        .all(Tag(*b"FNPR"))
        .nth(usize::from(marker))
        .and_then(|data| data.get(..2))
        .is_some_and(|data| u16::from_le_bytes([data[0], data[1]]) == marker_type))
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
    if matches!(function, 580 | 694) {
        if parameter != 0 {
            return Err(Error::Parameter(function, parameter));
        }
        let value = match function {
            580 => {
                let actor = world.actor(subject).ok_or(Error::Form(subject))?;
                actor.unconscious && !actor.dead
            }
            694 => {
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .no_bleedout_recovery
            }
            _ => return Err(Error::Function(function)),
        };
        return Ok(boolean(value));
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
        14 | 277 | 494 | 640 => {
            actor_values::name(parameter).ok_or(Error::ActorValue(parameter))?
        }
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

fn unavailable_combat_query(
    world: &mut World,
    owner: GlobalFormId,
    subject: GlobalFormId,
    c: &Condition,
) -> Result<f32, Error> {
    if world
        .reference(subject)
        .is_none_or(|reference| reference.deleted)
    {
        return Err(Error::Form(subject));
    }
    let parameter = c.parameters[0];
    match c.function {
        672..=674 | 701 => no_parameter(c)?,
        678 => {
            first_parameter_only(c)?;
            let actor = resolve_reference(world, owner, parameter)?;
            if world.actor(actor).is_none() {
                return Err(Error::Form(actor));
            }
        }
        697 => {
            first_parameter_only(c)?;
            resolve(world, owner, parameter, Some(*b"KYWD"))?;
        }
        _ => return Err(Error::Function(c.function)),
    }
    world.actor(subject).ok_or(Error::Form(subject))?;
    let reason = match c.function {
        672 => "the runtime has no live attack-state producer",
        673 => "MeleeHit records confirmed contacts, not the active animation state",
        674 => "the runtime does not track the actor's last hostile identity",
        678 => "no damage forecast contract is documented for this condition",
        697 => "the runtime has no active attack type to compare with the keyword",
        701 => {
            return Err(Error::UnavailableContractReason(
                c.function,
                "the local CK corpus does not define IsStaggered condition behavior",
            ));
        }
        _ => return Err(Error::Function(c.function)),
    };
    Err(Error::UnavailableState {
        function: c.function,
        reason,
    })
}

fn no_parameter_second(c: &Condition) -> Result<(), Error> {
    if c.parameters[1] == 0 {
        Ok(())
    } else {
        Err(Error::Parameter(c.function, c.parameters[1]))
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

#[cfg(test)]
mod ai_47_tests {
    use super::{evaluate_one, supports_function, Context, Error};
    use crate::{dialogue::Session, Game};
    use soe_esm::{
        records::gameplay::{Comparison, Condition},
        FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    };

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn fixture() -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
        for object in [0x900, 0x901] {
            plugin.push_top(
                Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
            );
        }
        plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"SCEN"), 0xA00));
        let mut order = LoadOrder::new();
        order.add("AI-47.esp", plugin).unwrap();
        Game::new(order)
    }

    fn condition(function: u16, parameter: u32) -> Condition {
        Condition {
            flags: 0,
            comparison: Comparison::Number(1.0),
            function,
            parameters: [parameter, 0],
            run_on: 0,
            reference: 0,
            parameter_3: -1,
            strings: [None, None],
        }
    }

    fn evaluate(game: &Game, subject: GlobalFormId, condition: &Condition) -> Result<bool, Error> {
        evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            Context {
                owner: id(0x800),
                subject,
                target: None,
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
                skill_use: None,
                light_level: None,
            },
            condition,
        )
    }

    #[test]
    fn ai_47_greeting_only_matches_the_force_greet_subject() {
        let game = fixture();
        game.world_mut().dialogue.greeting_actor = Some(id(0x900));
        assert_eq!(evaluate(&game, id(0x900), &condition(123, 0)), Ok(true));
        assert_eq!(evaluate(&game, id(0x901), &condition(123, 0)), Ok(false));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(123, 1)),
            Err(Error::Parameter(123, 1))
        );
    }

    #[test]
    fn ai_47_scene_playing_uses_valid_scene_identity_and_active_state() {
        let game = fixture();
        assert_eq!(
            evaluate(&game, id(0x900), &condition(248, 0xA00)),
            Ok(false)
        );
        game.world_mut().scenes.insert(id(0xA00));
        assert_eq!(evaluate(&game, id(0x900), &condition(248, 0xA00)), Ok(true));
        assert!(matches!(
            evaluate(&game, id(0x900), &condition(248, 0xFFFF)),
            Err(Error::Form(_))
        ));
        let mut bad_parameter = condition(248, 0xA00);
        bad_parameter.parameters[1] = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &bad_parameter),
            Err(Error::Parameter(248, 1))
        );
    }

    #[test]
    fn ai_47_dialogue_predicate_matches_player_pair_and_active_lifecycle() {
        let game = fixture();
        let player = game.world_mut().player();
        game.world_mut().dialogue.session = Some(Session {
            speaker: id(0x900),
            listener: player,
            topic: None,
            current_info: None,
            last_info: None,
            response_index: 0,
        });
        assert_eq!(evaluate(&game, id(0x900), &condition(249, 0)), Ok(true));
        assert_eq!(evaluate(&game, id(0x901), &condition(249, 0)), Ok(false));
        assert_eq!(evaluate(&game, id(0x910), &condition(249, 0)), Ok(false));
        game.world_mut().dialogue.session = None;
        assert_eq!(evaluate(&game, id(0x900), &condition(249, 0)), Ok(false));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(249, 1)),
            Err(Error::Parameter(249, 1))
        );
    }

    #[test]
    fn ai_47_unavailable_dialogue_contracts_return_explicit_diagnostics() {
        let game = fixture();
        for function in [50, 141, 172] {
            assert!(!supports_function(function), "function {function}");
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, 0)),
                Err(Error::Function(function)),
                "function {function}"
            );
            let mut missing_target = condition(function, 0);
            missing_target.run_on = 1;
            assert_eq!(
                evaluate(&game, id(0x900), &missing_target),
                Err(Error::Target(1)),
                "function {function}"
            );
        }
    }

    #[test]
    fn ai_47_persuasion_conditions_follow_the_dlg_07_speechcraft_contract() {
        let game = fixture();
        game.world_mut()
            .set_actor_value(id(0x900), "speechcraft", 40.0)
            .unwrap();
        for function in [225, 315] {
            assert!(supports_function(function));
            let mut matching = condition(function, 0);
            matching.comparison = Comparison::Number(40.0);
            assert_eq!(evaluate(&game, id(0x900), &matching), Ok(true));
            let mut wrong = condition(function, 0);
            wrong.comparison = Comparison::Number(41.0);
            assert_eq!(evaluate(&game, id(0x900), &wrong), Ok(false));
            let mut bad_parameter = condition(function, 0);
            bad_parameter.parameters[1] = 1;
            assert_eq!(
                evaluate(&game, id(0x900), &bad_parameter),
                Err(Error::Parameter(function, 1))
            );
        }
    }
}

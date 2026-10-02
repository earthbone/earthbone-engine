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
use soe_papyrus::{ObjectId, Value, Vm};
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
    #[error("condition function {0} requires a used-item action context")]
    UsedItemContext(u16),
    #[error("condition function {0} cannot read its equipment record")]
    EquipmentRecord(u16),
    #[error("condition count overflow or cyclic form list")]
    Inventory,
    #[error("condition function {0} has no usable query contract")]
    NoConditionContract(u16),
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
    let lookup = |object, name: String| vm.conditional_vars(object, &name);
    evaluate_with_vm(world, Some(&lookup), context, conditions)
}

/// Evaluate conditions using a callback-safe view of the VM's conditional variables.
pub(crate) fn evaluate_with_lookup(
    world: &mut World,
    context: Context,
    conditions: &[Condition],
    lookup: &dyn Fn(ObjectId, String) -> Vec<Value>,
) -> Result<bool, Error> {
    evaluate_with_vm(world, Some(lookup), context, conditions)
}

pub(crate) fn evaluate_without_vm(
    world: &mut World,
    context: Context,
    conditions: &[Condition],
) -> Result<bool, Error> {
    evaluate_with_vm(world, None, context, conditions)
}

fn evaluate_with_vm(
    world: &mut World,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    context: Context,
    conditions: &[Condition],
) -> Result<bool, Error> {
    let mut any = false;
    for (index, condition) in conditions.iter().enumerate() {
        if !any {
            any = evaluate_one_with_vm(world, lookup, context, condition)?;
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
            | 309
            | 128
            | 277
            | 408
            | 550
            | 430
            | 635
            | 494
            | 312
            | 314
            | 329
            | 353
            | 354
            | 362
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
            | 145
            | 152
            | 225
            | 315
            | 237
            | 242
            | 278
            | 300
            | 304
            | 403
            | 520
            | 566
            | 74
            | 84
            | 136
            | 182
            | 244
            | 264
            | 289
            | 323
            | 359
            | 338
            | 448
            | 543
            | 458
            | 459
            | 497
            | 499
            | 533
            | 534
            | 535
            | 560
            | 571
            | 579
            | 595
            | 596
            | 602
            | 652
            | 606
            | 627
            | 632
            | 629
            | 630
            | 41
            | 48
            | 102
            | 103
            | 108
            | 109
            | 246
            | 247
            | 639
            | 619
            | 623
            | 633
            | 654
            | 655
            | 656
            | 659
            | 254
            | 258
            | 259
            | 261
            | 365
            | 410
            | 42
            | 43
            | 44
            | 60
            | 68
            | 129
            | 130
            | 131
            | 132
            | 133
            | 134
            | 135
            | 577
            | 197
            | 199
            | 453
            | 615
            | 616
            | 617
            | 618
            | 641
            | 170
            | 716
            | 683
            | 698
            | 704
            | 675
            | 714
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
            | 358
            | 450
            | 584
            | 589
            | 594
            | 613
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
            | 1024
            | 1025
            | 1026
            | 1027
            | 1028
            | 193
            | 375
            | 376
            | 402
            | 263
            | 306
            | 398
            | 465
            | 597
            | 624
            | 157
            | 161
            | 203
            | 215
            | 650
            | 680
            | 682
            | 700
            | 720
            | 722
            | 725
            | 726
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
    let lookup = |object, name: String| vm.conditional_vars(object, &name);
    evaluate_one_with_vm(world, Some(&lookup), context, c)
}

fn evaluate_one_with_vm(
    world: &mut World,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    context: Context,
    c: &Condition,
) -> Result<bool, Error> {
    if c.flags & 0x0a != 0 {
        return Err(Error::Flags(c.flags & 0x1f));
    }
    if c.function == 726 {
        no_parameter(c)?;
        if c.run_on != 5 {
            return Err(Error::Contract(c.function));
        }
        let quest = context.quest.ok_or(Error::Target(5))?;
        let alias = u32::try_from(c.parameter_3).map_err(|_| Error::Target(5))?;
        if world
            .quest_state(quest)
            .and_then(|state| state.def.alias(alias))
            .is_none()
        {
            return Err(Error::Target(5));
        }
        if world.alias_ref(quest, alias).is_none() {
            let expected = expected_value(world, context.owner, c)?;
            return compare(boolean(true), expected, c);
        }
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
    let actual = function_value(world, lookup, context, subject, c)?;
    let expected = expected_value(world, context.owner, c)?;
    if !actual.is_finite() || !expected.is_finite() {
        return Err(Error::NonFinite);
    }
    compare(actual, expected, c)
}

fn expected_value(world: &mut World, owner: GlobalFormId, c: &Condition) -> Result<f32, Error> {
    Ok(match c.comparison {
        Comparison::Number(value) => value,
        Comparison::Global(raw) => {
            let global = resolve(world, owner, raw, Some(*b"GLOB"))?;
            world.global_value(global)
        }
    })
}

fn compare(actual: f32, expected: f32, c: &Condition) -> Result<bool, Error> {
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

fn actor_context(
    world: &mut World,
    subject: GlobalFormId,
) -> Result<&mut crate::world::ActorState, Error> {
    world.actor(subject).ok_or(Error::Form(subject))
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

/// xEdit's `wbMiscStatEnum` values and their matching `Game.QueryStat` names.
/// `GetPCMiscStat` stores the same integer values as the tracked-stat API.
fn misc_stat_name(value: u32) -> Option<&'static str> {
    Some(match value {
        0xFCDD5011 => "Animals Killed",
        0x366D84CF => "Armor Improved",
        0x023497E6 => "Armor Made",
        0x8E20D7C9 => "Assaults",
        0x579FFA75 => "Automations Killed",
        0xB9B50725 => "Backstabs",
        0xED6A0EF2 => "Barters",
        0xCCB952CE => "Books Read",
        0x317E8B4C => "Brawls Won",
        0x1D79006B => "Bribes",
        0x3602DE8F => "Bunnies Slaughtered",
        0x53D9E9B5 => "Chests Looted",
        0x683C1980 => "Civil War Quests Completed",
        0x66CCC50A => "College of Winterhold Quests Completed",
        0x40B11EFE => "Creatures Killed",
        0x22D5BA38 => "Critical Strikes",
        0xA930980F => "Daedra Killed",
        0x3558374B => "Daedric Quests Completed",
        0x37A76425 => "Dawnguard Quests Completed",
        0x2BDAC36F => "Days as a Vampire",
        0x6E684590 => "Days as a Werewolf",
        0xB6F118DB => "Days Jailed",
        0x3C626A90 => "Days Passed",
        0x8556AD88 => "Diseases Contracted",
        0x46D6FBBC => "Dragon Souls Collected",
        0xAA444695 => "Dungeons Cleared",
        0x1A37F336 => "Eastmarch Bounty",
        0x5AC3A8ED => "Falkreath Bounty",
        0x87B12ECC => "Favorite School",
        0x518BBC4E => "Favorite Shout",
        0x41DD77A6 => "Favorite Spell",
        0x171C5391 => "Favorite Weapon",
        0x4F041AA2 => "Fines Paid",
        0x9311B22B => "Food Eaten",
        0x57C089F7 => "Gold Found",
        0xD20EDA4F => "Haafingar Bounty",
        0x516C486D => "Hjaalmarch Bounty",
        0xB0A1E32E => "Horses Owned",
        0xEBAE35E8 => "Horses Stolen",
        0xFA024018 => "Hours Slept",
        0xCAD2ECA1 => "Hours Waiting",
        0x527DF857 => "Houses Owned",
        0x47B4A015 => "Ingredients Eaten",
        0xCE842356 => "Ingredients Harvested",
        0x7D2E57C0 => "Intimidations",
        0xC21702B5 => "Items Pickpocketed",
        0x82F190C2 => "Items Stolen",
        0x6627464B => "Jail Escapes",
        0x3520E710 => "Largest Bounty",
        0x8A24FDE2 => "Locations Discovered",
        0x5829CC2E => "Locks Picked",
        0x88089979 => "Magic Items Made",
        0x7EA26C2D => "Main Quests Completed",
        0x7187A208 => "Mauls",
        0x98EE55DC => "Misc Objectives Completed",
        0xFA06230B => "Most Gold Carried",
        0xD37C6909 => "Murders",
        0x22C2CBD0 => "Necks Bitten",
        0xBEEBCC87 => "Nirnroots Found",
        0x56CCFC54 => "NumVampirePerks",
        0x76A1A5C0 => "NumWerewolfPerks",
        0xF22A8133 => "People Killed",
        0x47A78467 => "Persuasions",
        0xF2BAC234 => "Pockets Picked",
        0x17C64668 => "Poisons Mixed",
        0x7D8F2EA6 => "Poisons Used",
        0x4228DE85 => "Potions Mixed",
        0x9631EC11 => "Potions Used",
        0xDE6C73FE => "Questlines Completed",
        0x0D7B8B16 => "Quests Completed",
        0xBB39399E => "Shouts Learned",
        0x731B5333 => "Shouts Mastered",
        0xF921D8BA => "Shouts Unlocked",
        0xB1AE4792 => "Side Quests Completed",
        0xACE470D7 => "Skill Books Read",
        0xF33130CE => "Skill Increases",
        0xB556CC52 => "Sneak Attacks",
        0xA74CBE83 => "Soul Gems Used",
        0xC2C9E233 => "Souls Trapped",
        0x5EC89F1A => "Spells Learned",
        0xB251A346 => "Standing Stones Found",
        0x05D45702 => "Stores Invested In",
        0xD0FE7031 => "The Companions Quests Completed",
        0x52BA68CB => "The Dark Brotherhood Quests Completed",
        0x3E267D77 => "The Pale Bounty",
        0x69B48177 => "The Reach Bounty",
        0x50A23F69 => "The Rift Bounty",
        0x62B2E95D => "Thieves' Guild Quests Completed",
        0x944CEA93 => "Times Jailed",
        0x50AAB633 => "Times Shouted",
        0x99BB86D8 => "Total Lifetime Bounty",
        0x4C252391 => "Training Sessions",
        0x7AEA9C2B => "Trespasses",
        0xA67626F4 => "Tribal Orcs Bounty",
        0x41D4BC0F => "Undead Killed",
        0xF39260A1 => "Vampirism Cures",
        0x61A5C5A9 => "Weapons Disarmed",
        0x1D3BA844 => "Weapons Improved",
        0x25F1EA25 => "Weapons Made",
        0x38A2DD66 => "Werewolf Transformations",
        0x4231FA4F => "Whiterun Bounty",
        0x92565767 => "Wings Plucked",
        0xC7FC518D => "Winterhold Bounty",
        0x949FA7BC => "Words of Power Learned",
        0x2C6E3FC0 => "Words of Power Unlocked",
        _ => return None,
    })
}

/// Rejects a value in either parameter slot of a function that takes none,
/// the same way functions 70 and 448 reject a slot they do not use.
fn no_parameter(c: &Condition) -> Result<(), Error> {
    match c.parameters {
        [0, 0] => Ok(()),
        [0, other] | [other, _] => Err(Error::Parameter(c.function, other)),
    }
}

fn one_parameter(c: &Condition) -> Result<(), Error> {
    if c.parameters[1] == 0 {
        Ok(())
    } else {
        Err(Error::Parameter(c.function, c.parameters[1]))
    }
}

fn first_parameter_only(c: &Condition) -> Result<(), Error> {
    no_parameter_second(c)
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
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
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
            one_parameter(c)?;
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
            one_parameter(c)?;
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
        735 | 358 | 450 | 584 | 589 | 613 => {
            return Err(Error::UnavailableContract(c.function));
        }
        1 => {
            let other = resolve_reference(world, context.owner, raw)?;
            world.distance(subject, other)
        }
        577 => {
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let first = resolve_reference(world, context.owner, raw)?;
            let second = resolve_reference(world, context.owner, c.parameters[1])?;
            boolean(world.distance(subject, first) < world.distance(subject, second))
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
        0 => {
            no_parameter(c)?;
            boolean(world.actor(subject).ok_or(Error::Form(subject))?.blocking)
        }
        5 => {
            no_parameter(c)?;
            boolean(world.reference(subject).ok_or(Error::Form(subject))?.locked)
        }
        32 => {
            one_parameter(c)?;
            let other = resolve_reference(world, context.owner, raw)?;
            let subject_cell = world.reference(subject).ok_or(Error::Form(subject))?.cell;
            let other_cell = world.reference(other).ok_or(Error::Form(other))?.cell;
            boolean(subject_cell.is_some() && subject_cell == other_cell)
        }
        65 => {
            no_parameter(c)?;
            world
                .reference(subject)
                .ok_or(Error::Form(subject))?
                .lock_level as f32
        }
        117 => {
            one_parameter(c)?;
            let region = resolve(world, context.owner, raw, Some(*b"REGN"))?;
            let player = world.player();
            let cell = world
                .reference(player)
                .ok_or(Error::Form(player))?
                .cell
                .ok_or(Error::Target(0))?;
            let cell_ref = world.order().winner_ref(cell).ok_or(Error::Form(cell))?;
            let record = world.order().record(cell_ref);
            let cell_data = soe_esm::records::cell::Cell::from_record(record, false)
                .map_err(|_| Error::Form(cell))?;
            boolean(
                cell_data
                    .regions
                    .iter()
                    .any(|raw_region| world.order().resolve(cell_ref.file, *raw_region) == region),
            )
        }
        146 => {
            no_parameter(c)?;
            let cell = world
                .reference(subject)
                .ok_or(Error::Form(subject))?
                .cell
                .ok_or(Error::Target(0))?;
            let cell_owner = world.cell_state(cell).ok_or(Error::Form(cell))?.owner;
            let owned = cell_owner.is_some_and(|owner| {
                world.base_of(subject) == Some(owner)
                    || world
                        .actor(subject)
                        .is_some_and(|actor| actor.factions.contains_key(&owner))
            });
            boolean(owned)
        }
        180 | 181 => {
            let (reference, keyword) = if c.function == 180 {
                (
                    resolve_reference(world, context.owner, raw)?,
                    resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?,
                )
            } else {
                let quest = context.quest.ok_or(Error::Target(5))?;
                let alias = world.alias_ref(quest, raw).ok_or(Error::Target(5))?;
                if world
                    .reference(alias)
                    .is_none_or(|reference| reference.deleted)
                {
                    return Err(Error::Form(alias));
                }
                (
                    alias,
                    resolve(world, context.owner, c.parameters[1], Some(*b"KYWD"))?,
                )
            };
            let subject_location = world.editor_location(subject);
            let other_location = world.editor_location(reference);
            boolean(subject_location.is_some_and(|location| {
                Some(location) == other_location && world.has_keyword(location, keyword)
            }))
        }
        650 => {
            let target = resolve_reference(world, context.owner, raw)?;
            let keyword = if c.parameters[1] == 0 {
                None
            } else {
                Some(resolve(
                    world,
                    context.owner,
                    c.parameters[1],
                    Some(*b"KYWD"),
                )?)
            };
            let reference = world.reference(subject).ok_or(Error::Form(subject))?;
            boolean(
                reference.linked.iter().any(|(linked_keyword, linked)| {
                    *linked == target && *linked_keyword == keyword
                }),
            )
        }
        680 | 725 => return Err(Error::Contract(c.function)),
        720 => {
            let target = resolve_reference(world, context.owner, raw)?;
            let subject_z = world
                .reference(subject)
                .filter(|reference| !reference.deleted)
                .ok_or(Error::Form(subject))?
                .position[2];
            let target_z = world
                .reference(target)
                .filter(|reference| !reference.deleted)
                .ok_or(Error::Form(target))?
                .position[2];
            target_z - subject_z
        }
        682 | 722 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let equipped: Vec<_> = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .equipped
                .iter()
                .copied()
                .collect();
            let count = equipped
                .into_iter()
                .filter(|item| {
                    world.tag_of(*item) == Some(Tag(*b"ARMO")) && world.has_keyword(*item, keyword)
                })
                .count();
            if c.function == 682 {
                boolean(count != 0)
            } else {
                count as f32
            }
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
            if matches!(
                world.tag_of(form),
                Some(Tag(tag)) if [*b"MGEF", *b"ENCH", *b"WOOP"].contains(&tag)
            ) {
                let player = world.player();
                boolean(
                    world
                        .actor(player)
                        .ok_or(Error::Form(player))?
                        .known_forms
                        .contains(&form),
                )
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
        700 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            boolean(
                world
                    .active_effects
                    .values()
                    .any(|effect| effect.target == subject && effect.restore_owner.is_some()),
            )
        }
        726 => 0.0,
        18 => world.calendar().hour as f32,
        170 => {
            no_parameter(c)?;
            let calendar = world.calendar();
            let month = usize::try_from(calendar.month).map_err(|_| Error::Form(subject))?;
            if month >= crate::clock::DAYS_IN_MONTH.len() || calendar.day == 0 {
                return Err(Error::Form(subject));
            }
            let days_before_year = i64::from(calendar.year).saturating_sub(201) * 365;
            let days_before_month = crate::clock::DAYS_IN_MONTH[..month]
                .iter()
                .map(|days| i64::from(*days))
                .sum::<i64>()
                - crate::clock::DAYS_IN_MONTH[..7]
                    .iter()
                    .map(|days| i64::from(*days))
                    .sum::<i64>();
            let days = days_before_year
                .saturating_add(days_before_month)
                .saturating_add(i64::from(calendar.day))
                .saturating_sub(17);
            days.rem_euclid(7) as f32
        }
        41 => {
            no_parameter(c)?;
            let items: Vec<_> = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .equipped
                .iter()
                .copied()
                .collect();
            let mut total = 0i32;
            for item in items {
                let Some(record) = world.record(item) else {
                    return Err(Error::EquipmentRecord(c.function));
                };
                if record.tag != Tag(*b"ARMO") {
                    continue;
                }
                let armor = crate::combat::records::Armor::from_record(record)
                    .map_err(|_| Error::EquipmentRecord(c.function))?;
                if armor.kind == Some(2) {
                    total = total
                        .checked_add(armor.value.ok_or(Error::EquipmentRecord(c.function))?)
                        .ok_or(Error::EquipmentRecord(c.function))?;
                }
            }
            total as f32
        }
        48 => {
            no_parameter(c)?;
            world.reference(subject).ok_or(Error::Form(subject))?;
            let gold = world.form("Gold001").unwrap_or(GlobalFormId {
                file: 0,
                object: 0xF,
            });
            inventory_count(world, subject, gold, &mut Vec::new())? as f32
        }
        46 => boolean(world.actor(subject).is_some_and(|actor| actor.dead)),
        42 | 43 | 44 | 60 => {
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            match c.function {
                42..=44 => {
                    let other = resolve_reference(world, context.owner, raw)?;
                    if world.actor(other).is_none() {
                        return Err(Error::Form(other));
                    }
                    match c.function {
                        42 => {
                            let first = world
                                .actor(subject)
                                .ok_or(Error::Form(subject))?
                                .factions
                                .clone();
                            let second = world
                                .actor(other)
                                .ok_or(Error::Form(other))?
                                .factions
                                .clone();
                            boolean(first.keys().any(|faction| second.contains_key(faction)))
                        }
                        43 => {
                            let first = world.actor(subject).and_then(|actor| actor.race);
                            let second = world.actor(other).and_then(|actor| actor.race);
                            boolean(first.is_some() && first == second)
                        }
                        _ => {
                            let (_, first) = world
                                .actor_npc_part(subject, crate::npc::template::TRAITS)
                                .map_err(|_| Error::Form(subject))?;
                            let (_, second) = world
                                .actor_npc_part(other, crate::npc::template::TRAITS)
                                .map_err(|_| Error::Form(other))?;
                            boolean(first.is_female() == second.is_female())
                        }
                    }
                }
                _ => {
                    let faction = resolve(world, context.owner, raw, Some(*b"FACT"))?;
                    let actor = resolve_reference(world, context.owner, c.parameters[1])?;
                    if world.actor(actor).is_none() {
                        return Err(Error::Form(actor));
                    }
                    let subject_rank = world
                        .actor(subject)
                        .and_then(|state| state.factions.get(&faction).copied())
                        .unwrap_or(-2);
                    let other_rank = world
                        .actor(actor)
                        .and_then(|state| state.factions.get(&faction).copied())
                        .unwrap_or(-2);
                    (i64::from(subject_rank) - i64::from(other_rank)) as f32
                }
            }
        }
        12 | 62 | 75 | 148 | 153 | 154 => {
            no_parameter(c)?;
            return Err(Error::Function(c.function));
        }
        149 => {
            resolve(world, context.owner, raw, Some(*b"WTHR"))?;
            return Err(Error::Function(c.function));
        }
        64 | 143 | 150 | 228 | 226 | 227 | 449 | 454 | 637 | 703 | 715 => {
            return Err(Error::Function(c.function));
        }
        641 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.actor_base(base).ok_or(Error::Form(base))?.unique)
        }
        68 => {
            let class = resolve(world, context.owner, raw, Some(*b"CLAS"))?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.actor_base(base).and_then(|data| data.class) == Some(class))
        }
        129 | 130 => {
            let expected_tag = if c.function == 129 {
                *b"CLAS"
            } else {
                *b"RACE"
            };
            let expected = resolve(world, context.owner, raw, Some(expected_tag))?;
            let player = world.player();
            if world.actor(player).is_none() {
                return Err(Error::Form(player));
            }
            let base = world.base_of(player).ok_or(Error::Form(player))?;
            let actual = world.actor_base(base).and_then(|data| {
                if c.function == 129 {
                    data.class
                } else {
                    data.race
                }
            });
            boolean(actual == Some(expected))
        }
        131 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            if raw > 1 {
                return Err(Error::Parameter(c.function, raw));
            }
            let player = world.player();
            let (_, traits) = world
                .actor_npc_part(player, crate::npc::template::TRAITS)
                .map_err(|_| Error::Form(player))?;
            boolean(traits.is_female() == (raw == 1))
        }
        132 | 197 | 199 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let faction = resolve(world, context.owner, raw, Some(*b"FACT"))?;
            match c.function {
                132 => {
                    let player = world.player();
                    if world.actor(player).is_none() {
                        return Err(Error::Form(player));
                    }
                    boolean(
                        world
                            .actor(player)
                            .is_some_and(|actor| actor.factions.contains_key(&faction)),
                    )
                }
                197 => boolean(
                    world
                        .faction(faction)
                        .ok_or(Error::Form(faction))?
                        .player_enemy,
                ),
                _ => boolean(
                    world
                        .faction(faction)
                        .ok_or(Error::Form(faction))?
                        .player_attacked,
                ),
            }
        }
        133..=135 => {
            no_parameter(c)?;
            let player = world.player();
            if world.actor(subject).is_none() || world.actor(player).is_none() {
                return Err(Error::Form(if world.actor(subject).is_none() {
                    subject
                } else {
                    player
                }));
            }
            match c.function {
                133 => {
                    let subject_factions = world
                        .actor(subject)
                        .ok_or(Error::Form(subject))?
                        .factions
                        .clone();
                    let player_factions = world
                        .actor(player)
                        .ok_or(Error::Form(player))?
                        .factions
                        .clone();
                    boolean(
                        subject_factions
                            .keys()
                            .any(|faction| player_factions.contains_key(faction)),
                    )
                }
                134 => {
                    let subject_race = world.actor(subject).and_then(|actor| actor.race);
                    let player_race = world.actor(player).and_then(|actor| actor.race);
                    boolean(subject_race.is_some() && subject_race == player_race)
                }
                _ => {
                    let (_, subject_traits) = world
                        .actor_npc_part(subject, crate::npc::template::TRAITS)
                        .map_err(|_| Error::Form(subject))?;
                    let (_, player_traits) = world
                        .actor_npc_part(player, crate::npc::template::TRAITS)
                        .map_err(|_| Error::Form(player))?;
                    boolean(subject_traits.is_female() == player_traits.is_female())
                }
            }
        }
        237 | 242 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(if c.function == 237 {
                actor.ghost
            } else {
                actor.unconscious
            })
        }
        278 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            if world.reference(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let expected_owner = if raw == 0 {
                let player = world.player();
                world.base_of(player).ok_or(Error::Form(player))?
            } else {
                let owner = resolve(world, context.owner, raw, None)?;
                if !matches!(world.tag_of(owner), Some(Tag(tag)) if tag == *b"NPC_" || tag == *b"FACT")
                {
                    return Err(Error::Form(owner));
                }
                owner
            };
            boolean(
                world
                    .reference(subject)
                    .and_then(|reference| reference.owner)
                    == Some(expected_owner),
            )
        }
        300 => {
            no_parameter(c)?;
            let cell = world.reference(subject).ok_or(Error::Form(subject))?.cell;
            boolean(cell.is_some_and(|cell| world.cell_is_interior(cell)))
        }
        84 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let base = resolve(world, context.owner, raw, Some(*b"NPC_"))?;
            world.actor_base(base).ok_or(Error::Form(base))?.dead_count as f32
        }
        102 | 103 => {
            no_parameter(c)?;
            let items: Vec<_> = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .equipped
                .iter()
                .copied()
                .collect();
            let mut equipped = false;
            for item in items {
                let tag = world.tag_of(item);
                if c.function == 102 && is_equipped_torch(world, item) {
                    equipped = true;
                    break;
                }
                if c.function == 103 && tag == Some(Tag(*b"ARMO")) {
                    let record = world
                        .record(item)
                        .ok_or(Error::EquipmentRecord(c.function))?;
                    let armor = crate::combat::records::Armor::from_record(record)
                        .map_err(|_| Error::EquipmentRecord(c.function))?;
                    if armor.slots & 0x200 != 0 {
                        equipped = true;
                        break;
                    }
                }
            }
            boolean(equipped)
        }
        108 | 109 => {
            let (right, left) = {
                let actor = world.actor(subject).ok_or(Error::Form(subject))?;
                (actor.right_weapon, actor.left_weapon)
            };
            let equipped = right.or(left);
            let Some(weapon) = equipped else {
                if c.function == 108 {
                    no_parameter(c)?;
                    return Ok(0.0);
                }
                if c.parameters[1] != 0 {
                    return Err(Error::Parameter(c.function, c.parameters[1]));
                }
                return Ok(0.0);
            };
            let record = world
                .record(weapon)
                .ok_or(Error::EquipmentRecord(c.function))?;
            let data = crate::combat::records::Weapon::from_record(record)
                .map_err(|_| Error::EquipmentRecord(c.function))?;
            if c.function == 108 {
                no_parameter(c)?;
                f32::from(data.animation)
            } else {
                if c.parameters[1] != 0 {
                    return Err(Error::Parameter(c.function, c.parameters[1]));
                }
                boolean(data.skill >= 0 && data.skill as u32 == raw)
            }
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
            one_parameter(c)?;
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
            one_parameter(c)?;
            let wanted = resolve(world, context.owner, raw, Some(*b"WRLD"))?;
            boolean(reference_worldspace(world, subject) == Some(wanted))
        }
        325 => {
            one_parameter(c)?;
            package_location_condition(world, subject, c, raw)?
        }
        445 => {
            one_parameter(c)?;
            let zone = resolve(world, context.owner, raw, Some(*b"ECZN"))?;
            let subject_zone = world
                .encounter_zone(subject)
                .map_err(|_| Error::Form(subject))?;
            boolean(subject_zone == Some(zone))
        }
        446 => {
            one_parameter(c)?;
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
            one_parameter(c)?;
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
            one_parameter(c)?;
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            let location = world
                .current_location(subject)
                .ok_or(Error::LocationContext(subject))?;
            boolean(world.keywords(location).contains(&keyword))
        }
        563 => {
            one_parameter(c)?;
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
            one_parameter(c)?;
            let location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            boolean(world.editor_location(subject) == Some(location))
        }
        567 => {
            one_parameter(c)?;
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
            one_parameter(c)?;
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
            one_parameter(c)?;
            let _location = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            return Err(Error::Unavailable(c.function));
        }
        626 => {
            one_parameter(c)?;
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
            one_parameter(c)?;
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
        1024..=1027 => {
            no_parameter(c)?;
            0.0
        }
        1028 => {
            no_parameter(c)?;
            return Err(Error::NoConditionContract(c.function));
        }
        145 => {
            // CK IsTrespassing - Actor defines this as the subject's current
            // trespassing state. The world retains that state per actor.
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .trespassing,
            )
        }
        152 => {
            // Actor.GetCrimeFaction returns the faction this actor reports
            // crimes to. GetIsCrimeFaction tests that same relationship.
            let faction = resolve(world, context.owner, raw, Some(*b"FACT"))?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .crime_faction
                    == Some(faction),
            )
        }
        193 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let faction = resolve(world, context.owner, raw, Some(*b"FACT"))?;
            boolean(
                world
                    .faction(faction)
                    .ok_or(Error::Form(faction))?
                    .player_expelled,
            )
        }
        // CONTEXT: CK has no usable query contract for these functions. The
        // runtime also lacks faction murder counts, stolen-item totals and
        // the horse-theft flag, so do not infer values from bounty or inventory.
        101 | 195 | 282 | 366 | 373 | 479 | 487 | 552 => {
            return Err(Error::Contract(c.function));
        }
        375 | 376 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let faction = if raw == 0 {
                world
                    .actor(subject)
                    .and_then(|actor| actor.crime_faction)
                    .ok_or(Error::Form(subject))?
            } else {
                resolve(world, context.owner, raw, Some(*b"FACT"))?
            };
            let state = world.faction(faction).ok_or(Error::Form(faction))?;
            if c.function == 375 {
                state.crime_gold_violent as f32
            } else {
                state.crime_gold_nonviolent as f32
            }
        }
        402 => {
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
            let Some(completed) = world.scene_action_slot_complete(scene, slot) else {
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
        654 => {
            // GetBribeSuccess observes the player's bribe outcome. The CK
            // SetBribed/IsBribed contract stores bribed and intimidated
            // outcomes in LastBribedIntimidated, which is also used by
            // IsIntimidatedbyPlayer (function 116).
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
        309 => {
            // CONTEXT: xEdit declares no parameters. The CK corpus has no page
            // for this legacy platform predicate. This runtime targets PC, so
            // its Xbox query is false.
            no_parameter(c)?;
            0.0
        }
        520 => {
            // Skyrim Open Engine does not run on the PlayStation 3. xEdit
            // identifies this condition as parameterless.
            no_parameter(c)?;
            0.0
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
        244 => {
            no_parameter(c)?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.restrained && !actor.unconscious)
        }
        304 => {
            // IsWaterObject asks whether the tested reference's base is WATR.
            no_parameter(c)?;
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.tag_of(base) == Some(Tag(*b"WATR")))
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
        458 => {
            // The CK corpus has no page for this condition. Witness memory
            // records perpetrators, so the player is an enemy after this
            // subject witnesses the player's crime.
            no_parameter(c)?;
            let player = world.player();
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .witnessed_crimes
                    .contains(&player),
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
        459 | 533 | 534 | 535 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            // xEdit marks the faction parameter nullable. A zero parameter
            // uses the subject's own crime faction, as named by GetCrimeFaction.
            let faction = if raw == 0 {
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .crime_faction
                    .ok_or(Error::Form(subject))?
            } else {
                resolve(world, context.owner, raw, Some(*b"FACT"))?
            };
            let state = world.faction(faction).ok_or(Error::Form(faction))?;
            (match c.function {
                459 => state
                    .crime_gold_violent
                    .saturating_add(state.crime_gold_nonviolent),
                533 => state.infamy_violent.saturating_add(state.infamy_nonviolent),
                534 => state.infamy_violent,
                _ => state.infamy_nonviolent,
            }) as f32
        }
        497 => {
            no_parameter(c)?;
            let faction = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .crime_faction
                .ok_or(Error::Form(subject))?;
            let state = world.faction(faction).ok_or(Error::Form(faction))?;
            let owed = state
                .crime_gold_violent
                .saturating_add(state.crime_gold_nonviolent)
                .max(0);
            let player = world.player();
            let gold = world
                .form("Gold001")
                .map_or(0, |gold| world.item_count(player, gold));
            boolean(gold >= owed)
        }
        499 => {
            no_parameter(c)?;
            let player = world.player();
            world.actor(player).ok_or(Error::Form(player))?.days_in_jail as f32
        }
        652 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let other = resolve_reference(world, context.owner, raw)?;
            let subject_faction = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .crime_faction;
            let other_faction = world.actor(other).ok_or(Error::Form(other))?.crime_faction;
            boolean(subject_faction.is_some() && subject_faction == other_faction)
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
        453 => {
            no_parameter(c)?;
            boolean(world.actor(subject).ok_or(Error::Form(subject))?.teammate)
        }
        615 | 616 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            let ranks = world.authored_relationship_ranks(base);
            (match c.function {
                615 => ranks.into_iter().max().unwrap_or(0),
                _ => ranks.into_iter().min().unwrap_or(0),
            }) as f32
        }
        617 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let association = resolve(world, context.owner, raw, Some(*b"ASTP"))?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.actor_has_association_type(base, association))
        }
        618 => {
            no_parameter(c)?;
            if world.actor(subject).is_none() {
                return Err(Error::Form(subject));
            }
            let base = world.base_of(subject).ok_or(Error::Form(subject))?;
            boolean(world.actors_have_family_relationship(base, None))
        }
        566 => {
            let quest = context.quest.ok_or(Error::Target(5))?;
            boolean(world.alias_ref(quest, raw) == Some(subject))
        }
        571 => {
            no_parameter_second(c)?;
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
            no_parameter_second(c)?;
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
            no_parameter_second(c)?;
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
        246 | 247 => {
            world.actor(subject).ok_or(Error::Form(subject))?;
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            if c.function == 246 {
                let _ = resolve(world, context.owner, raw, None)?;
            }
            return Err(Error::UsedItemContext(c.function));
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
                    lookup
                        .map(|lookup| lookup(object, name.to_string()))
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
        367 | 368 | 390 | 576 | 646 | 647 => return Err(Error::Function(c.function)),
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
        157 => {
            no_parameter(c)?;
            let (base, opened) = world
                .reference(subject)
                .filter(|reference| !reference.deleted)
                .map(|reference| (reference.base, reference.opened))
                .ok_or(Error::Form(subject))?;
            let is_door = base.is_some_and(|base| world.tag_of(base) == Some(Tag(*b"DOOR")));
            if !is_door {
                0.0
            } else if opened {
                1.0
            } else {
                3.0
            }
        }
        161 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let package = resolve(world, context.owner, raw, Some(*b"PACK"))?;
            let actor = world.actor(subject).ok_or(Error::Form(subject))?;
            boolean(actor.ai.package == Some(package))
        }
        203 => {
            no_parameter(c)?;
            boolean(
                world
                    .reference(subject)
                    .filter(|reference| !reference.deleted)
                    .ok_or(Error::Form(subject))?
                    .destroyed,
            )
        }
        215 => {
            no_parameter(c)?;
            if world
                .reference(subject)
                .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Form(subject));
            }
            let record = world.record(subject).ok_or(Error::Form(subject))?;
            boolean(record.find(Tag(*b"ONAM")).is_some())
        }
        182 => {
            let item = resolve(world, context.owner, raw, None)?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.equipped.contains(&item)),
            )
        }
        263 => {
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .weapon_drawn,
            )
        }
        306 => {
            no_parameter(c)?;
            let equipped = world
                .actor(subject)
                .ok_or(Error::Form(subject))?
                .equipped
                .iter()
                .copied()
                .collect::<Vec<_>>();
            boolean(
                equipped
                    .into_iter()
                    .any(|item| is_equipped_torch(world, item)),
            )
        }
        398 => {
            let list = resolve(world, context.owner, raw, Some(*b"FLST"))?;
            let weapons = world.actor(subject).ok_or(Error::Form(subject))?;
            let hands = [weapons.left_weapon, weapons.right_weapon];
            let forms = world.form_list(list).cloned().ok_or(Error::Form(list))?;
            boolean(
                hands
                    .into_iter()
                    .flatten()
                    .any(|weapon| forms.contains(&weapon)),
            )
        }
        465 => {
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            inventory_count(world, subject, keyword, &mut Vec::new())? as f32
        }
        597 => {
            if raw > 1 {
                return Err(Error::Parameter(c.function, raw));
            }
            equipped_item_type(world, subject, raw == 0)? as f32
        }
        624 => {
            let container = resolve_reference(world, context.owner, raw)?;
            let base = world.base_of(subject).unwrap_or(subject);
            boolean(world.item_count(container, base) > 0)
        }
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
        312 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let name = misc_stat_name(raw).ok_or(Error::Parameter(c.function, raw))?;
            world.stats.get(name).copied().unwrap_or(0) as f32
        }
        314 => {
            no_parameter(c)?;
            boolean(world.is_actor_victim(subject))
        }
        329 => {
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .is_some_and(|actor| actor.ai.procedure == crate::ai::Procedure::Flee),
            )
        }
        353 => {
            no_parameter(c)?;
            boolean(world.actor(subject).is_some())
        }
        354 => {
            no_parameter(c)?;
            boolean(world.actor(subject).is_some_and(|actor| actor.essential))
        }
        362 => {
            if c.parameters[1] != 0 {
                return Err(Error::Parameter(c.function, c.parameters[1]));
            }
            let keyword = resolve(world, context.owner, raw, Some(*b"KYWD"))?;
            boolean(world.reference(subject).is_some_and(|reference| {
                reference
                    .linked
                    .iter()
                    .any(|(linked_keyword, _)| *linked_keyword == Some(keyword))
            }))
        }
        359 => {
            let wanted = resolve(world, context.owner, raw, Some(*b"LCTN"))?;
            boolean(world.current_location(subject).is_some_and(|current| {
                current == wanted || world.location_is_child(wanted, current)
            }))
        }
        360 => {
            one_parameter(c)?;
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
            boolean(world.has_perk(subject, perk))
        }
        447 => {
            no_parameter_second(c)?;
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
        594 => {
            no_parameter(c)?;
            boolean(world.actor(subject).ok_or(Error::Form(subject))?.flying)
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
        619 => {
            let axis = raw
                .checked_sub(88)
                .and_then(|axis| usize::try_from(axis).ok())
                .ok_or(Error::Parameter(c.function, raw))?;
            if axis >= 3 {
                return Err(Error::Parameter(c.function, raw));
            }
            actor_context(world, subject)?;
            Err(Error::Contract(c.function))?
        }
        623 => {
            no_parameter(c)?;
            actor_context(world, subject)?;
            Err(Error::Contract(c.function))?
        }
        633 => {
            no_parameter(c)?;
            let actor = actor_context(world, subject)?;
            match actor.flying_state {
                Some(state @ 0..=4) => f32::from(state),
                Some(_) => return Err(Error::StateContract(c.function)),
                None => return Err(Error::Unavailable(c.function)),
            }
        }
        444 => {
            one_parameter(c)?;
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
                    lookup
                        .map(|lookup| lookup(object, name.to_string()))
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
            // yet (the native is still a stub in natives.rs). Do not approximate
            // the authored perk formula with a Speechcraft and Confidence threshold.
            no_parameter(c)?;
            let subject_actor = world.actor(subject).ok_or(Error::Form(subject))?;
            if !subject_actor.values.contains("confidence") {
                // No AIDT subrecord on the speaker's NPC_ record means the
                // condition has no usable actor-value contract for this form.
                return Err(Error::StateContract(c.function));
            }
            return Err(Error::Unavailable(c.function));
        }
        656 => {
            // CK Actor.IsArrested reports the actor's current arrest state.
            no_parameter(c)?;
            boolean(world.actor(subject).ok_or(Error::Form(subject))?.arrested)
        }
        659 => boolean(context.tempering_enchanted.ok_or(Error::TemperingContext)?),

        716 => {
            no_parameter(c)?;
            (world.clock.real_seconds / 3600.0) as f32
        }

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
        714 => {
            no_parameter(c)?;
            boolean(
                world
                    .actor(subject)
                    .ok_or(Error::Form(subject))?
                    .being_ridden,
            )
        }
        644 | 684 | 730 => return Err(Error::Unavailable(c.function)),
        other => return Err(Error::Function(other)),
    })
}

fn is_equipped_torch(world: &World, item: GlobalFormId) -> bool {
    let Some(record) = world
        .record(item)
        .filter(|record| record.tag == Tag(*b"LIGH"))
    else {
        return false;
    };
    let Some(flags) = record
        .find(Tag(*b"DATA"))
        .and_then(|data| data.get(12..16))
        .and_then(|data| data.try_into().ok())
        .map(u32::from_le_bytes)
    else {
        return false;
    };
    flags & 2 != 0
}

fn equipped_item_type(
    world: &mut World,
    actor: GlobalFormId,
    left_hand: bool,
) -> Result<u8, Error> {
    let state = world.actor(actor).ok_or(Error::Form(actor))?;
    let (spell, weapon) = if left_hand {
        (state.left_spell, state.left_weapon)
    } else {
        (state.right_spell, state.right_weapon)
    };
    if spell.is_some() {
        return Ok(9);
    }
    let Some(weapon) = weapon else {
        return Ok(0);
    };
    let Some(record) = world.record(weapon).filter(|record| !record.is_deleted()) else {
        return Err(Error::Form(weapon));
    };
    match record.tag.as_bytes() {
        b"WEAP" => {
            let animation = record
                .find(Tag(*b"DNAM"))
                .and_then(|data| data.first())
                .copied()
                .ok_or(Error::Form(weapon))?;
            match animation {
                0 => Ok(0),
                1..=4 => Ok(animation),
                5 => Ok(5),
                6 => Ok(6),
                7 => Ok(7),
                8 => Ok(8),
                9 => Ok(12),
                _ => Err(Error::Form(weapon)),
            }
        }
        b"ARMO" => {
            let slots = record
                .find(Tag(*b"BOD2"))
                .or_else(|| record.find(Tag(*b"BODT")))
                .and_then(|data| data.get(..4))
                .and_then(|data| data.try_into().ok())
                .map(u32::from_le_bytes)
                .ok_or(Error::Form(weapon))?;
            Ok(if slots & (1 << 8) != 0 { 10 } else { 0 })
        }
        b"LIGH" => Ok(if is_equipped_torch(world, weapon) {
            11
        } else {
            0
        }),
        b"SCRL" | b"SPEL" => Ok(9),
        _ => Err(Error::Form(weapon)),
    }
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

/// Approximate the player's intimidation result using Speechcraft and target
/// Confidence. The perk entry point that supplies the complete formula is
/// not implemented yet.
pub(crate) fn will_intimidate_succeed(
    world: &mut World,
    subject: GlobalFormId,
) -> Result<bool, Error> {
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
        Ok(speechcraft >= confidence * 25.0)
    } else {
        Ok(false)
    }
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
mod ai_condition_tests {
    use super::*;
    use crate::{dialogue::Session, quest, Game};
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
        plugin.push_top(Record::new(Tag(*b"STAT"), 0x910));
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 0x902).sub(Tag(*b"NAME"), 0x910_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"KYWD"), 0x820));
        plugin.push_top(
            Record::new(Tag(*b"ARMO"), 0x830).sub(Tag(*b"KWDA"), 0x820_u32.to_le_bytes()),
        );
        plugin.push_top(quest::with_alias(
            Record::new(Tag(*b"QUST"), 0x840),
            7,
            "Target",
            None,
        ));
        let mut order = LoadOrder::new();
        order.add("AI-47-78.esp", plugin).unwrap();
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

    fn condition_full(function: u16, first: u32, second: u32, expected: f32) -> Condition {
        Condition {
            flags: 0,
            comparison: Comparison::Number(expected),
            function,
            parameters: [first, second],
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
                quest: Some(id(0x840)),
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

    #[test]
    fn ai_78_is_linked_to_matches_reference_and_keyword() {
        let game = fixture();
        game.world_mut()
            .reference(id(0x900))
            .unwrap()
            .linked
            .push((Some(id(0x820)), id(0x901)));
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(650, 0x901, 0x820, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(650, 0x902, 0x820, 1.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(650, 0x901, 0x820, 0.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(650, 0x910, 0x820, 1.0)),
            Err(Error::Form(id(0x910)))
        );
    }

    #[test]
    fn ai_78_get_activator_height_reports_missing_contract() {
        let game = fixture();
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(680, 0, 0, 0.0)),
            Err(Error::Contract(680))
        );
    }

    #[test]
    fn ai_78_worn_keyword_checks_and_counts_apparel() {
        let game = fixture();
        game.world_mut()
            .actor(id(0x900))
            .unwrap()
            .equipped
            .insert(id(0x830));
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(682, 0x820, 0, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(682, 0x820, 0, 0.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(722, 0x820, 0, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(722, 0x820, 0, 2.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, id(0x902), &condition_full(682, 0x820, 0, 0.0)),
            Err(Error::Form(id(0x902)))
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(682, 0x910, 0, 1.0)),
            Err(Error::Form(id(0x910)))
        );
    }

    #[test]
    fn ai_65_graph_float_sprint_and_flight_conditions_match_runtime_state() {
        let game = fixture();
        let actor = id(0x900);
        let mut graph = condition(447, 0);
        graph.strings[0] = Some("SpeedBlend".into());
        game.world_mut()
            .actor(actor)
            .unwrap()
            .graph_floats
            .insert("speedblend".into(), 1.25);
        graph.comparison = Comparison::Number(1.25);
        assert!(supports_function(447));
        assert_eq!(evaluate(&game, actor, &graph), Ok(true));
        graph.comparison = Comparison::Number(0.75);
        assert_eq!(evaluate(&game, actor, &graph), Ok(false));
        let mut malformed = graph.clone();
        malformed.parameters[1] = 1;
        assert_eq!(
            evaluate(&game, actor, &malformed),
            Err(Error::Parameter(447, 1))
        );
        let mut missing_graph = graph;
        missing_graph.strings[0] = Some("Missing".into());
        assert!(matches!(
            evaluate(&game, actor, &missing_graph),
            Err(Error::Variable(_))
        ));

        let mut sprint = condition(568, 0);
        game.world_mut().actor(actor).unwrap().sprinting = true;
        assert!(supports_function(568));
        assert_eq!(evaluate(&game, actor, &sprint), Ok(true));
        game.world_mut().actor(actor).unwrap().sprinting = false;
        assert_eq!(evaluate(&game, actor, &sprint), Ok(false));
        sprint.parameters[0] = 1;
        assert_eq!(
            evaluate(&game, actor, &sprint),
            Err(Error::Parameter(568, 1))
        );

        let mut flying = condition(594, 0);
        game.world_mut().actor(actor).unwrap().flying = true;
        assert!(supports_function(594));
        assert_eq!(evaluate(&game, actor, &flying), Ok(true));
        game.world_mut().actor(actor).unwrap().flying = false;
        assert_eq!(evaluate(&game, actor, &flying), Ok(false));
        flying.parameters[0] = 1;
        assert_eq!(
            evaluate(&game, actor, &flying),
            Err(Error::Parameter(594, 1))
        );
        assert!(matches!(
            evaluate(&game, id(0x910), &condition(594, 0)),
            Err(Error::Form(_))
        ));
    }

    #[test]
    fn ai_78_commanded_actor_reads_live_command_effect_ownership() {
        let game = fixture();
        let query = condition_full(700, 0, 0, 1.0);
        assert_eq!(evaluate(&game, id(0x900), &query), Ok(false));
        game.world_mut().make_commanded_for_test(id(0x900));
        assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
        assert_eq!(evaluate(&game, id(0x901), &query), Ok(false));
        assert_eq!(
            evaluate(&game, id(0x902), &query),
            Err(Error::Form(id(0x902)))
        );
    }

    #[test]
    fn ai_78_target_height_returns_signed_vertical_delta() {
        let game = fixture();
        game.world_mut().reference(id(0x900)).unwrap().position[2] = 4.0;
        game.world_mut().reference(id(0x901)).unwrap().position[2] = 9.0;
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(720, 0x901, 0, 5.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(720, 0x901, 0, -5.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(720, 0x910, 0, 0.0)),
            Err(Error::Form(id(0x910)))
        );
    }

    #[test]
    fn ai_78_knocked_state_reports_missing_contract() {
        let game = fixture();
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(725, 0, 0, 0.0)),
            Err(Error::Contract(725))
        );
    }

    #[test]
    fn ai_78_does_not_exist_checks_unfilled_quest_alias() {
        let game = fixture();
        let mut missing = condition_full(726, 0, 0, 1.0);
        missing.run_on = 5;
        missing.parameter_3 = 7;
        assert_eq!(evaluate(&game, id(0x900), &missing), Ok(true));
        game.world_mut().force_alias(id(0x840), 7, Some(id(0x900)));
        assert_eq!(evaluate(&game, id(0x900), &missing), Ok(false));
        assert_eq!(
            evaluate(&game, id(0x900), &condition_full(726, 0, 0, 0.0)),
            Err(Error::Contract(726))
        );
        let mut invalid = missing;
        invalid.parameter_3 = 99;
        assert_eq!(evaluate(&game, id(0x900), &invalid), Err(Error::Target(5)));
    }

    #[test]
    fn ai_65_undocumented_motion_conditions_return_explicit_contract_diagnostics() {
        let game = fixture();
        for function in [358, 450, 584, 589, 613] {
            assert!(supports_function(function), "recognized CTDA {function}");
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, 0)),
                Err(Error::UnavailableContract(function)),
                "function {function}"
            );
            assert_eq!(
                evaluate(&game, id(0x910), &condition(function, 0)),
                Err(Error::UnavailableContract(function)),
                "function {function} diagnostic does not assume a context"
            );
        }
        for function in [447, 568] {
            let mut predicate = condition(function, 0);
            if function == 447 {
                predicate.strings[0] = Some("SpeedBlend".into());
            }
            assert!(
                matches!(evaluate(&game, id(0x910), &predicate), Err(Error::Form(_))),
                "function {function} requires an actor"
            );
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Gameplay CTDA execution. Field/function identities come from xEdit
//! wbConditions/wbConditionFunctions. CK documents the state queries and
//! conditional Papyrus variables. See docs/game.md for the implemented set.

use crate::{inventory::Field, Handle, World};
use soe_esm::{
    records::gameplay::{Comparison, Condition},
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
        1 | 14
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
            | 237
            | 242
            | 278
            | 300
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

/// Rejects a value in either parameter slot of a function that takes none,
/// the same way functions 70 and 448 reject a slot they do not use.
fn no_parameter(c: &Condition) -> Result<(), Error> {
    match c.parameters {
        [0, 0] => Ok(()),
        [0, other] | [other, _] => Err(Error::Parameter(c.function, other)),
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
        14 | 128 | 277 | 408 | 430 | 494 => {
            actor_query(world, context.owner, subject, c.function, raw)?
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
        18 => world.calendar().hour as f32,
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
        64 | 228 => return Err(Error::Function(c.function)),
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
        67 => {
            let cell = resolve(world, context.owner, raw, Some(*b"CELL"))?;
            boolean(
                world
                    .reference(subject)
                    .and_then(|reference| reference.cell)
                    == Some(cell),
            )
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

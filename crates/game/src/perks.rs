// SPDX-License-Identifier: GPL-3.0-or-later

//! Player PERK purchases using xEdit's top-level CTDA and NNAM rank chain.
//! Existing actor perk identities remain authoritative. Effect entries have
//! separate conditions and are outside the purchase transaction.
//!
//! DATA Trait, Level and Num Ranks are accepted at any value (PRG-16). Trait
//! and Level are never set on an installed winning PERK record and no source
//! in the corpus describes a runtime consumer for either byte, so they carry
//! no purchase meaning here. Num Ranks is a Creation Kit display field kept
//! in sync with the actual NNAM chain, so the chain walked by [`rank_chain`]
//! is the count that matters, not the byte. Hidden perks purchase the same
//! as shown ones, this API has no UI layer for the hidden flag to affect.
//!
//! DATA also accepts the two truncated encodings wrye-bash's
//! `MelPerkData(MelTruncatedStruct)` declares in `old_versions={'4B', '3B'}`
//! (`Mopy/bash/brec/common_subrecords.py`, reused unmodified for Skyrim in
//! `Mopy/bash/game/skyrim/records.py`): a 4-byte DATA carries Trait, Level,
//! Num Ranks and Playable with Hidden defaulted to 0, and a 3-byte DATA
//! carries only Trait, Level and Num Ranks with Playable and Hidden both
//! defaulted to 0, matching `MelTruncatedStruct`'s zero-padding of the
//! fields a shorter format omits. A record in either shorter, documented
//! encoding is readable, not malformed.
//! See docs/game.md, "Ordinary perk purchases", for the sources read.

use crate::{conditions, Game, World};
use soe_esm::{records::gameplay::Condition, GlobalFormId, Tag};
use soe_papyrus::{ObjectId, Value, Vm};
use std::collections::{BTreeMap, BTreeSet};

/// A failed purchase leaves perk ownership and the point balance unchanged.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("perk {0} is missing, deleted or malformed")]
    Perk(GlobalFormId),
    #[error("perk {0} is not playable")]
    NotPlayable(GlobalFormId),
    #[error("perk {0} is not the first record in a rank chain")]
    NotRoot(GlobalFormId),
    #[error("perk rank chain contains a cycle at {0}")]
    Cycle(GlobalFormId),
    #[error("all ranks are already owned")]
    Complete,
    #[error("no perk points are available")]
    NoPoints,
    #[error("player actor is unavailable")]
    Player,
    #[error("perk purchase prerequisites are false")]
    Ineligible,
    #[error("perk condition format: {0}")]
    Format(#[from] soe_esm::Error),
    #[error("perk condition: {0}")]
    Condition(#[from] conditions::Error),
}

/// The exact record acquired and its one-based position in the NNAM chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purchase {
    pub perk: GlobalFormId,
    pub rank: usize,
}

/// Runtime context for one complex PERK entry point. Tab subjects follow the
/// tab order authored by the entry point, with tab zero usually being its owner.
#[derive(Debug, Clone)]
pub struct EntryPointContext {
    pub tabs: Vec<GlobalFormId>,
    pub target: Option<GlobalFormId>,
    pub quest: Option<GlobalFormId>,
    pub tempering_item: Option<GlobalFormId>,
    /// Poison selection for alchemy perk entry points.
    pub alchemy_making_poison: Option<bool>,
    /// Effect currently checked by alchemy perk entry points.
    pub alchemy_effect: Option<GlobalFormId>,
    /// Skill-use inputs for EPModSkillUsage entry point conditions.
    pub skill_use: Option<conditions::SkillUseContext>,
}

/// The decoded fields needed by an entry point handler.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryPointInvocation {
    pub actor: GlobalFormId,
    pub perk: GlobalFormId,
    /// PRKE's stored rank plus one, where zero denotes rank one.
    pub rank: u16,
    pub priority: u8,
    pub function: u8,
    pub data_type: Option<u8>,
    pub data: Vec<u8>,
}

/// Errors raised while dispatching a complex PERK entry point.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DispatchError {
    #[error("entry point {0} has no registered handler")]
    Unregistered(u8),
    #[error("perk {0} has malformed entry point data")]
    Malformed(GlobalFormId),
    #[error("perk {perk} entry point uses missing tab {tab}")]
    MissingTab { perk: GlobalFormId, tab: u8 },
    #[error("perk {0} entry point has unsupported section type")]
    UnsupportedSection(GlobalFormId),
    #[error("perk {0} entry point condition: {1}")]
    Condition(GlobalFormId, #[source] conditions::Error),
    #[error("perk {0} entry point format: {1}")]
    Format(GlobalFormId, #[source] soe_esm::Error),
}

pub type Handler = fn(&mut World, &EntryPointInvocation);

/// Registered handlers for numeric Creation Kit perk entry points.
#[derive(Default)]
pub struct EntryPointDispatcher {
    handlers: BTreeMap<u8, Handler>,
}

impl EntryPointDispatcher {
    /// Register or replace the handler for an entry point ID.
    pub fn register(&mut self, entry_point: u8, handler: Handler) {
        self.handlers.insert(entry_point, handler);
    }

    /// Evaluate rank-eligible entries by winning-record, ascending priority,
    /// then source order. Conditions are evaluated before their handlers run.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed sections, missing tab context,
    /// unsupported condition functions or an unregistered entry point.
    pub fn dispatch(
        &self,
        game: &mut Game,
        actor: GlobalFormId,
        entry_point: u8,
        context: &EntryPointContext,
    ) -> Result<usize, DispatchError> {
        let handler = self
            .handlers
            .get(&entry_point)
            .ok_or(DispatchError::Unregistered(entry_point))?;
        let mut world = game.world_mut();
        let invocations =
            entry_point_invocations(&mut world, &game.vm, actor, entry_point, context)?;
        for invocation in &invocations {
            handler(&mut world, invocation);
        }
        Ok(invocations.len())
    }
}

/// Collect eligible entry-point operations by winning-record, ascending
/// priority, then source order. Priority direction and cross-perk ordering
/// have not been confirmed against the reference runtime.
///
/// # Errors
/// Malformed sections, missing tab subjects and unsupported conditions.
pub fn entry_point_invocations(
    world: &mut World,
    vm: &soe_papyrus::Vm,
    actor: GlobalFormId,
    entry_point: u8,
    context: &EntryPointContext,
) -> Result<Vec<EntryPointInvocation>, DispatchError> {
    entry_point_invocations_with_conditions(world, Some(vm), None, actor, entry_point, context)
}

/// Collect eligible entries using a caller-provided view of conditional VM variables.
pub(crate) fn entry_point_invocations_with_lookup(
    world: &mut World,
    actor: GlobalFormId,
    entry_point: u8,
    context: &EntryPointContext,
    lookup: &dyn Fn(ObjectId, String) -> Vec<Value>,
) -> Result<Vec<EntryPointInvocation>, DispatchError> {
    entry_point_invocations_with_conditions(world, None, Some(lookup), actor, entry_point, context)
}

fn entry_point_invocations_with_conditions(
    world: &mut World,
    vm: Option<&Vm>,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    actor: GlobalFormId,
    entry_point: u8,
    context: &EntryPointContext,
) -> Result<Vec<EntryPointInvocation>, DispatchError> {
    let mut owned = world.perks_for_actor(actor);
    let npc_ranks = actor_npc_perk_ranks(world, actor)?;
    owned.extend(npc_ranks.keys().copied());
    let entries = world
        .order()
        .winners_of(Tag(*b"PERK"))
        .filter(|(perk, record)| owned.contains(perk) && !record.is_deleted())
        .map(|(perk, record)| entry_point_sections(record, perk).map(|sections| (perk, sections)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut invocations = Vec::new();
    for (perk_id, sections) in entries {
        let rank = npc_ranks
            .get(&perk_id)
            .copied()
            .map_or_else(|| actor_perk_rank(world, &owned, perk_id), Ok)?;
        let mut matching = Vec::new();
        for section in sections {
            if section.entry_point != entry_point {
                continue;
            }
            if u16::from(section.rank) + 1 > rank {
                continue;
            }
            if !entry_conditions(world, vm, lookup, perk_id, &section, context)? {
                continue;
            }
            let invocation = EntryPointInvocation {
                actor,
                perk: perk_id,
                rank: u16::from(section.rank) + 1,
                priority: section.priority,
                function: section.function,
                data_type: section.data_type,
                data: section.data,
            };
            matching.push(invocation);
        }
        matching.sort_by_key(|invocation| invocation.priority);
        invocations.extend(matching);
    }
    Ok(invocations)
}

/// Apply numeric functions from ordered value-type invocations.
///
/// # Errors
/// Returns an error for malformed operation data or non-finite results.
pub(crate) fn apply_value_modifiers(
    world: &mut World,
    invocations: &[EntryPointInvocation],
    mut value: f32,
) -> Result<f32, DispatchError> {
    for invocation in invocations {
        let function = invocation.function;
        let amount = match function {
            1..=3 => {
                if invocation.data_type != Some(1) || invocation.data.len() != 4 {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                f32::from_le_bytes(
                    invocation
                        .data
                        .as_slice()
                        .try_into()
                        .map_err(|_| DispatchError::Malformed(invocation.perk))?,
                )
            }
            4 => {
                if invocation.data_type != Some(2) || invocation.data.len() != 8 {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                let min = f32::from_le_bytes(
                    invocation.data[0..4]
                        .try_into()
                        .map_err(|_| DispatchError::Malformed(invocation.perk))?,
                );
                let max = f32::from_le_bytes(
                    invocation.data[4..8]
                        .try_into()
                        .map_err(|_| DispatchError::Malformed(invocation.perk))?,
                );
                if !min.is_finite() || !max.is_finite() {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                world.random_float(min, max)
            }
            5 | 12..=14 => {
                if invocation.data_type != Some(2) || invocation.data.len() != 8 {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                let raw_index = f32::from_le_bytes(
                    invocation.data[0..4]
                        .try_into()
                        .map_err(|_| DispatchError::Malformed(invocation.perk))?,
                );
                let coefficient = f32::from_le_bytes(
                    invocation.data[4..8]
                        .try_into()
                        .map_err(|_| DispatchError::Malformed(invocation.perk))?,
                );
                if !raw_index.is_finite() || raw_index.fract() != 0.0 || !coefficient.is_finite() {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                let index = i32::try_from(raw_index as i64)
                    .map_err(|_| DispatchError::Malformed(invocation.perk))?;
                let name = crate::actor_values::actor_value_name(index)
                    .ok_or(DispatchError::Malformed(invocation.perk))?;
                world.actor_value(invocation.actor, name) * coefficient
            }
            6 | 7 => {
                if invocation.data_type.is_some() || !invocation.data.is_empty() {
                    return Err(DispatchError::Malformed(invocation.perk));
                }
                0.0
            }
            _ => return Err(DispatchError::Unregistered(function)),
        };
        if !amount.is_finite() {
            return Err(DispatchError::Malformed(invocation.perk));
        }
        value = match function {
            1 | 12 => amount,
            2 | 4 | 5 => value + amount,
            3 | 13 => value * amount,
            6 => value.abs(),
            7 => -value.abs(),
            14 => value * (1.0 + amount),
            _ => value,
        };
        if !value.is_finite() {
            return Err(DispatchError::Malformed(invocation.perk));
        }
    }
    Ok(value)
}

fn actor_npc_perk_ranks(
    world: &mut World,
    actor: GlobalFormId,
) -> Result<BTreeMap<GlobalFormId, u16>, DispatchError> {
    let Some(base) = world.base_of(actor) else {
        return Ok(BTreeMap::new());
    };
    let Some(record) = world.record(base) else {
        return Ok(BTreeMap::new());
    };
    let mut ranks = BTreeMap::new();
    for subrecord in record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.tag == Tag(*b"PRKR"))
    {
        let Ok(bytes) = <[u8; 8]>::try_from(subrecord.data.as_slice()) else {
            return Err(DispatchError::Malformed(base));
        };
        let raw_perk = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let Some(perk) = world.resolve_from(base, raw_perk) else {
            continue;
        };
        let rank = u16::from(bytes[4]);
        if ranks.insert(perk, rank).is_some() {
            return Err(DispatchError::Malformed(base));
        }
    }
    Ok(ranks)
}

fn actor_perk_rank(
    world: &World,
    owned: &BTreeSet<GlobalFormId>,
    perk: GlobalFormId,
) -> Result<u16, DispatchError> {
    let records = world
        .order()
        .winners_of(Tag(*b"PERK"))
        .filter(|(_, record)| !record.is_deleted())
        .collect::<Vec<_>>();
    let mut predecessors = BTreeMap::<GlobalFormId, Vec<GlobalFormId>>::new();
    let mut successors = BTreeMap::<GlobalFormId, GlobalFormId>::new();
    for (id, record) in &records {
        let next_fields = record
            .subrecords
            .iter()
            .filter(|sub| sub.tag == Tag(*b"NNAM"))
            .collect::<Vec<_>>();
        if next_fields.len() > 1 {
            if *id == perk {
                return Err(DispatchError::Malformed(*id));
            }
            continue;
        }
        let Some(field) = next_fields.first() else {
            continue;
        };
        let Ok(bytes) = <[u8; 4]>::try_from(field.data.as_slice()) else {
            if *id == perk {
                return Err(DispatchError::Malformed(*id));
            }
            continue;
        };
        let raw = u32::from_le_bytes(bytes);
        if raw == 0 {
            continue;
        }
        let Some(next) = world.resolve_from(*id, raw) else {
            if *id == perk {
                return Err(DispatchError::Malformed(*id));
            }
            continue;
        };
        successors.insert(*id, next);
        predecessors.entry(next).or_default().push(*id);
    }

    let mut root = perk;
    let mut seen = BTreeSet::new();
    while let Some(previous) = predecessors.get(&root) {
        if previous.len() != 1 || !seen.insert(root) {
            return Err(DispatchError::Malformed(perk));
        }
        root = previous[0];
    }
    let mut cursor = root;
    let mut actor_rank = 0u16;
    let mut position = 0u16;
    seen.clear();
    while seen.insert(cursor) {
        position = position.saturating_add(1);
        if owned.contains(&cursor) {
            actor_rank = position;
        }
        let Some(next) = successors.get(&cursor) else {
            break;
        };
        cursor = *next;
    }
    if seen.contains(&cursor) && successors.contains_key(&cursor) {
        return Err(DispatchError::Malformed(perk));
    }
    if actor_rank == 0 {
        return Err(DispatchError::Malformed(perk));
    }
    Ok(actor_rank)
}

struct EntrySection {
    complex: bool,
    has_data: bool,
    entry_point: u8,
    rank: u8,
    priority: u8,
    function: u8,
    tab_count: u8,
    tabs: Vec<(u8, Vec<Condition>)>,
    data_type: Option<u8>,
    data: Vec<u8>,
}

fn entry_point_sections(
    record: &soe_esm::Record,
    perk: GlobalFormId,
) -> Result<Vec<EntrySection>, DispatchError> {
    let mut sections = Vec::new();
    let mut current: Option<EntrySection> = None;
    let mut tab: Option<(u8, Vec<Condition>)> = None;
    for sub in &record.subrecords {
        match sub.tag.as_bytes() {
            b"PRKE" => {
                if current.is_some() {
                    return Err(DispatchError::Malformed(perk));
                }
                finish_tab(&mut current, &mut tab);
                finish_section(&mut sections, &mut current);
                if sub.data.len() != 3 {
                    return Err(DispatchError::Malformed(perk));
                }
                if sub.data[0] > 2 {
                    return Err(DispatchError::UnsupportedSection(perk));
                }
                if sub.data[0] != 2 {
                    current = Some(EntrySection {
                        complex: false,
                        has_data: false,
                        entry_point: u8::MAX,
                        rank: sub.data[1],
                        priority: sub.data[2],
                        function: 0,
                        tab_count: 0,
                        tabs: Vec::new(),
                        data_type: None,
                        data: Vec::new(),
                    });
                    continue;
                }
                current = Some(EntrySection {
                    complex: true,
                    has_data: false,
                    entry_point: u8::MAX,
                    rank: sub.data[1],
                    priority: sub.data[2],
                    function: 0,
                    tab_count: 0,
                    tabs: Vec::new(),
                    data_type: None,
                    data: Vec::new(),
                });
            }
            b"DATA" if current.is_some() => {
                if !current.as_ref().is_some_and(|section| section.complex) {
                    continue;
                }
                if sub.data.len() != 3 {
                    return Err(DispatchError::Malformed(perk));
                }
                let section = current.as_mut().ok_or(DispatchError::Malformed(perk))?;
                if section.has_data {
                    return Err(DispatchError::Malformed(perk));
                }
                section.has_data = true;
                section.entry_point = sub.data[0];
                section.function = sub.data[1];
                section.tab_count = sub.data[2];
            }
            b"PRKC" if current.is_some() => {
                if !current.as_ref().is_some_and(|section| section.complex) {
                    continue;
                }
                finish_tab(&mut current, &mut tab);
                if sub.data.len() != 1 {
                    return Err(DispatchError::Malformed(perk));
                }
                tab = Some((sub.data[0], Vec::new()));
            }
            b"CTDA" | b"CIS1" | b"CIS2" if current.is_some() => {
                let conditions = &mut tab.as_mut().ok_or(DispatchError::Malformed(perk))?.1;
                Condition::push(record, sub, conditions)
                    .map_err(|error| DispatchError::Format(perk, error))?;
            }
            b"EPFT" if current.is_some() => {
                let section = current.as_mut().ok_or(DispatchError::Malformed(perk))?;
                if sub.data.len() != 1 || section.data_type.replace(sub.data[0]).is_some() {
                    return Err(DispatchError::Malformed(perk));
                }
            }
            b"EPFD" if current.is_some() => {
                current
                    .as_mut()
                    .ok_or(DispatchError::Malformed(perk))?
                    .data
                    .extend_from_slice(&sub.data);
            }
            b"PRKF" => {
                if current.is_none() {
                    return Err(DispatchError::Malformed(perk));
                }
                finish_tab(&mut current, &mut tab);
                finish_section(&mut sections, &mut current);
            }
            _ => {}
        }
    }
    if current.is_some() {
        return Err(DispatchError::Malformed(perk));
    }
    finish_tab(&mut current, &mut tab);
    finish_section(&mut sections, &mut current);
    if sections
        .iter()
        .any(|section| section.complex && !section.has_data)
    {
        return Err(DispatchError::Malformed(perk));
    }
    Ok(sections)
}

fn finish_tab(current: &mut Option<EntrySection>, tab: &mut Option<(u8, Vec<Condition>)>) {
    if let (Some(section), Some(tab)) = (current.as_mut(), tab.take()) {
        section.tabs.push(tab);
    }
}

fn finish_section(sections: &mut Vec<EntrySection>, current: &mut Option<EntrySection>) {
    if let Some(section) = current.take() {
        sections.push(section);
    }
}

fn entry_conditions(
    world: &mut World,
    vm: Option<&Vm>,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    perk: GlobalFormId,
    section: &EntrySection,
    context: &EntryPointContext,
) -> Result<bool, DispatchError> {
    if section.tabs.len() != usize::from(section.tab_count) {
        return Err(DispatchError::Malformed(perk));
    }
    if section.entry_point == u8::MAX {
        return Ok(false);
    }
    for (tab, conditions) in &section.tabs {
        let subject = context
            .tabs
            .get(usize::from(*tab))
            .copied()
            .ok_or(DispatchError::MissingTab { perk, tab: *tab })?;
        let condition_context = conditions::Context {
            owner: perk,
            subject,
            target: context.target,
            quest: context.quest,
            tempering_enchanted: None,
            tempering_item: context.tempering_item,
            alchemy_making_poison: context.alchemy_making_poison,
            alchemy_effect: context.alchemy_effect,
            magic_item: None,
            dual_cast: None,
            skill_use: context.skill_use,
            light_level: None,
        };
        let result = match (vm, lookup) {
            (Some(vm), _) => conditions::evaluate(world, vm, condition_context, conditions),
            (_, Some(lookup)) => {
                conditions::evaluate_with_lookup(world, condition_context, conditions, lookup)
            }
            (None, None) => conditions::evaluate_without_vm(world, condition_context, conditions),
        };
        if !result.map_err(|error| DispatchError::Condition(perk, error))? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct Perk {
    playable: bool,
    next: Option<GlobalFormId>,
    conditions: Vec<Condition>,
}

impl Perk {
    fn read(world: &World, id: GlobalFormId) -> Result<Self, Error> {
        let record = world
            .record(id)
            .filter(|r| r.tag == Tag(*b"PERK") && !r.is_deleted())
            .ok_or(Error::Perk(id))?;
        let mut data = None;
        let mut next = None;
        let mut conditions = Vec::new();
        for sub in &record.subrecords {
            match sub.tag.as_bytes() {
                b"PRKE" => break,
                b"DATA" => {
                    if data.is_some() {
                        return Err(Error::Perk(id));
                    }
                    data = Some(pad_legacy_data(&sub.data).ok_or(Error::Perk(id))?);
                }
                b"NNAM" => {
                    if next.is_some() {
                        return Err(Error::Perk(id));
                    }
                    let bytes = sub
                        .data
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::Perk(id))?;
                    next = Some(u32::from_le_bytes(bytes));
                }
                _ => {
                    Condition::push(record, sub, &mut conditions)?;
                }
            }
        }
        let data = data.ok_or(Error::Perk(id))?;
        let next = next
            .filter(|raw| *raw != 0)
            .map(|raw| world.resolve_from(id, raw).ok_or(Error::Perk(id)))
            .transpose()?;
        Ok(Self {
            playable: data[3] != 0 && record.flags & 4 == 0,
            next,
            conditions,
        })
    }
}

impl Game {
    /// Preview the first unowned rank starting at a chain's first PERK record.
    /// Script grants can leave gaps. Existing ownership is retained in that case.
    ///
    /// # Errors
    ///
    /// Rejects non-root inputs, invalid chains, exhausted points, completed
    /// chains, non-playable ranks and false prerequisites.
    pub fn check_perk_purchase(&mut self, root: GlobalFormId) -> Result<Purchase, Error> {
        let mut world = self.world.borrow_mut();
        let chain = rank_chain(&world, root)?;
        if world.perk_points <= 0 {
            return Err(Error::NoPoints);
        }
        let player = world.player();
        let owned = &world.actor(player).ok_or(Error::Player)?.perks;
        let (index, (id, perk)) = chain
            .iter()
            .enumerate()
            .find(|(_, (id, _))| !owned.contains(id))
            .ok_or(Error::Complete)?;
        if !perk.playable {
            return Err(Error::NotPlayable(*id));
        }
        let context = conditions::Context {
            owner: *id,
            subject: player,
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
        };
        if !conditions::evaluate(&mut world, &self.vm, context, &perk.conditions)? {
            return Err(Error::Ineligible);
        }
        Ok(Purchase {
            perk: *id,
            rank: index + 1,
        })
    }

    /// Acquire one rank and spend one player perk point after rechecking eligibility.
    /// Lower rank identities remain owned. This does not execute perk effects.
    ///
    /// # Errors
    ///
    /// Returns the same errors as check_perk_purchase without spending points.
    pub fn purchase_perk(&mut self, root: GlobalFormId) -> Result<Purchase, Error> {
        let purchase = self.check_perk_purchase(root)?;
        let mut world = self.world.borrow_mut();
        let player = world.player();
        world
            .actor(player)
            .ok_or(Error::Player)?
            .perks
            .insert(purchase.perk);
        world.perk_points -= 1;
        Ok(purchase)
    }
}

fn rank_chain(world: &World, root: GlobalFormId) -> Result<Vec<(GlobalFormId, Perk)>, Error> {
    let mut seen = BTreeSet::new();
    let mut chain = Vec::new();
    let mut next = Some(root);
    while let Some(id) = next {
        if !seen.insert(id) {
            return Err(Error::Cycle(id));
        }
        let perk = Perk::read(world, id)?;
        next = perk.next;
        chain.push((id, perk));
    }
    // CONTEXT: The API accepts a tree node's first rank, preventing a caller
    // from skipping earlier ranks by passing a later NNAM target as a root.
    for (id, record) in world.order().winners_of(Tag(*b"PERK")) {
        if record.is_deleted() {
            continue;
        }
        let raw = record
            .find(Tag(*b"NNAM"))
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes);
        if raw.is_some_and(|raw| raw != 0 && world.resolve_from(id, raw) == Some(root)) {
            return Err(Error::NotRoot(root));
        }
    }
    Ok(chain)
}

/// Accepts PERK DATA's full 5-byte encoding (Trait, Level, Num Ranks,
/// Playable, Hidden) plus the two truncated encodings wrye-bash's
/// `MelPerkData` declares as `old_versions={'4B', '3B'}`: a 4-byte DATA
/// omits Hidden and a 3-byte DATA omits Playable and Hidden too. Missing
/// trailing bytes are zero-padded, matching `MelTruncatedStruct`'s default.
/// Any other length is not a documented encoding and is rejected.
fn pad_legacy_data(bytes: &[u8]) -> Option<[u8; 5]> {
    if bytes.len() < 3 || bytes.len() > 5 {
        return None;
    }
    let mut padded = [0u8; 5];
    padded[..bytes.len()].copy_from_slice(bytes);
    Some(padded)
}

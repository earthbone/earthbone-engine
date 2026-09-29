// SPDX-License-Identifier: GPL-3.0-or-later

//! Routes story events through winning SMEN, SMBN and SMQN records.

use std::collections::BTreeSet;

use soe_esm::{records::gameplay::Condition, GlobalFormId, Tag};
use soe_papyrus::{vm::state::Writer, Value, Vm, VmError};

use super::save::{read_form_id, write_form_id, FormReader};
use super::save_sections::Section;
use super::{Alloc, Pending, World, T};
use crate::conditions::{self, Context};
use crate::quest::state::{RunData, StoryEvent};

const NODE_TAGS: [Tag; 3] = [Tag(*b"SMEN"), Tag(*b"SMBN"), Tag(*b"SMQN")];

enum StoryAllocator<'a> {
    Vm(&'a mut Vm),
    Native(Alloc<'a>),
}

impl StoryAllocator<'_> {
    fn allocate(&mut self, name: &str, class: &str) -> soe_papyrus::ObjectId {
        match self {
            Self::Vm(vm) => vm.create_object_of(name, class),
            Self::Native(alloc) => alloc(name, class),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoryError {
    #[error("malformed story node {0}")]
    Malformed(GlobalFormId),
    #[error("story condition on node {0}: {1}")]
    Condition(GlobalFormId, String),
    #[error("invalid story reset data on quest {0}")]
    Reset(GlobalFormId),
}

#[derive(Debug)]
struct Node {
    id: GlobalFormId,
    tag: Tag,
    parent: Option<GlobalFormId>,
    previous: Option<GlobalFormId>,
    flags: u32,
    max_active: usize,
    event: Option<u32>,
    conditions: Vec<Condition>,
}

#[derive(Debug)]
struct QuestChoice {
    quest: GlobalFormId,
    hours: f64,
}

impl World {
    /// Route one authored story event. Returns the quests started in tree order.
    /// Event parameters are retained on each quest and forwarded to its matching
    /// Papyrus `OnStory...` event after the normal `OnInit` startup event.
    pub fn send_story_event(
        &mut self,
        vm: &mut Vm,
        event: &StoryEvent,
    ) -> Result<Vec<GlobalFormId>, StoryError> {
        self.send_story_event_inner(event, &mut StoryAllocator::Vm(vm))
    }

    /// Route a native story event without reading Papyrus variables.
    pub(crate) fn send_story_event_without_vm(
        &mut self,
        event: &StoryEvent,
        alloc: Alloc<'_>,
    ) -> Result<Vec<GlobalFormId>, StoryError> {
        self.send_story_event_inner(event, &mut StoryAllocator::Native(alloc))
    }

    fn send_story_event_inner(
        &mut self,
        event: &StoryEvent,
        allocator: &mut StoryAllocator<'_>,
    ) -> Result<Vec<GlobalFormId>, StoryError> {
        let code = u32::from_le_bytes(event.code);
        let mut nodes = Vec::new();
        for (id, rref) in self.order.winners() {
            let rec = self.order.record(rref);
            if !NODE_TAGS.contains(&rec.tag) || rec.is_deleted() {
                continue;
            }
            let mut node = Node {
                id,
                tag: rec.tag,
                parent: None,
                previous: None,
                flags: 0,
                max_active: usize::MAX,
                event: None,
                conditions: Vec::new(),
            };
            for sub in &rec.subrecords {
                let data = sub.data.as_slice();
                match sub.tag.as_bytes() {
                    b"PNAM" | b"SNAM" if data.len() >= 4 => {
                        let raw = u32::from_le_bytes(
                            data[..4]
                                .try_into()
                                .map_err(|_| StoryError::Malformed(id))?,
                        );
                        let resolved = self.order.resolve(rref.file, raw);
                        if sub.tag == T(b"PNAM") {
                            node.parent = (raw != 0).then_some(resolved);
                        } else {
                            node.previous = (raw != 0).then_some(resolved);
                        }
                    }
                    b"DNAM" if data.len() >= 4 => {
                        node.flags = u32::from_le_bytes(
                            data[..4]
                                .try_into()
                                .map_err(|_| StoryError::Malformed(id))?,
                        );
                    }
                    b"XNAM" if data.len() >= 4 => {
                        node.max_active = u32::from_le_bytes(
                            data[..4]
                                .try_into()
                                .map_err(|_| StoryError::Malformed(id))?,
                        ) as usize;
                    }
                    b"ENAM" if data.len() >= 4 => {
                        node.event = Some(u32::from_le_bytes(
                            data[..4]
                                .try_into()
                                .map_err(|_| StoryError::Malformed(id))?,
                        ));
                    }
                    _ => {
                        Condition::push(rec, sub, &mut node.conditions)
                            .map_err(|_| StoryError::Malformed(id))?;
                    }
                }
            }
            nodes.push(node);
        }
        nodes.sort_by_key(|node| node.id);

        let parameters = event
            .parameters
            .get(usize::from(event.code == *b"SCRP")..)
            .unwrap_or_default();
        let subject = parameters
            .iter()
            .find_map(|parameter| match parameter {
                crate::quest::state::StoryParameter::Reference {
                    reference: Some(id),
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap_or(self.player());
        let event_roots: Vec<_> = nodes
            .iter()
            .filter(|node| node.tag == T(b"SMEN") && node.event == Some(code))
            .map(|node| node.id)
            .collect();
        let mut started = Vec::new();
        for root in event_roots {
            self.route_node(
                &nodes,
                root,
                event,
                subject,
                usize::MAX,
                &mut started,
                allocator,
            )?;
        }
        Ok(started)
    }

    fn route_node(
        &mut self,
        nodes: &[Node],
        parent: GlobalFormId,
        event: &StoryEvent,
        subject: GlobalFormId,
        budget: usize,
        started: &mut Vec<GlobalFormId>,
        allocator: &mut StoryAllocator<'_>,
    ) -> Result<(), StoryError> {
        let started_before = started.len();
        let Some(node) = nodes.iter().find(|node| node.id == parent) else {
            return Ok(());
        };
        if !self.story_conditions(allocator, node, subject)? {
            return Ok(());
        }
        let active_descendants = self
            .descendant_quests(nodes, parent)?
            .into_iter()
            .filter(|quest| self.quest_state(*quest).is_some_and(|state| state.running))
            .count();
        let own_capacity = if node.max_active == 0 {
            usize::MAX
        } else {
            node.max_active.saturating_sub(active_descendants)
        };
        let budget = budget.min(own_capacity);
        if budget == 0 {
            return Ok(());
        }
        if node.tag == T(b"SMQN") {
            let choices = self.quest_choices(node.id)?;
            let active = choices
                .iter()
                .filter(|choice| self.quest_state(choice.quest).is_some_and(|q| q.running))
                .count();
            let limit = if node.max_active == 0 {
                usize::MAX
            } else {
                node.max_active
            };
            let mut eligible = Vec::new();
            for choice in choices {
                let now = self.clock.game_days;
                let cooldown = self
                    .story_history
                    .get(&choice.quest)
                    .map_or(0.0, |(_, until)| *until);
                let already_run = self
                    .story_history
                    .get(&choice.quest)
                    .is_some_and(|(count, _)| *count > 0);
                let run_once = self
                    .quest_state(choice.quest)
                    .is_some_and(|q| q.def.flags & crate::quest::flags::RUN_ONCE != 0);
                if active >= limit
                    || (choice.hours > 0.0 && now < cooldown)
                    || (already_run && run_once)
                    || self.quest_state(choice.quest).is_some_and(|q| q.running)
                {
                    continue;
                }
                if self.quest_state(choice.quest).is_none() {
                    continue;
                }
                eligible.push(choice);
            }
            let active = active.min(limit);
            let capacity = limit.saturating_sub(active).min(budget);
            if node.flags & (1 << 16) != 0 {
                let selected = self.story_selection.entry(node.id).or_default();
                eligible.retain(|choice| !selected.contains(&choice.quest));
                if eligible.is_empty() {
                    self.story_selection.remove(&node.id);
                    eligible = self
                        .quest_choices(node.id)?
                        .into_iter()
                        .filter(|choice| {
                            let cooldown = self
                                .story_history
                                .get(&choice.quest)
                                .map_or(0.0, |(_, until)| *until);
                            let already_run = self
                                .story_history
                                .get(&choice.quest)
                                .is_some_and(|(count, _)| *count > 0);
                            let run_once = self
                                .quest_state(choice.quest)
                                .is_some_and(|q| q.def.flags & crate::quest::flags::RUN_ONCE != 0);
                            !(self.quest_state(choice.quest).is_none_or(|q| q.running)
                                || (already_run && run_once)
                                || (choice.hours > 0.0 && self.clock.game_days < cooldown))
                        })
                        .collect();
                }
            }
            if node.flags & 1 != 0 && !eligible.is_empty() {
                let index = self.random_int(0, eligible.len() as i32 - 1) as usize;
                eligible = vec![eligible.swap_remove(index)];
            }
            let requested = if node.flags & (1 << 18) != 0 {
                self.node_u32(node.id, T(b"MNAM")).unwrap_or(1) as usize
            } else {
                eligible.len()
            };
            for choice in eligible.into_iter().take(requested.min(capacity)) {
                let now = self.clock.game_days;
                let starts = self
                    .story_history
                    .get(&choice.quest)
                    .map_or(0, |(count, _)| *count)
                    .saturating_add(1);
                let reset_at = if choice.hours > 0.0 {
                    now + choice.hours / 24.0
                } else {
                    0.0
                };
                let Some(q) = self.quest_state(choice.quest) else {
                    continue;
                };
                q.runtime.run_data = Some(RunData {
                    event_flag: 1,
                    event: Some(event.clone()),
                    ..RunData::default()
                });
                let mut alloc = |name: &str, class: &str| allocator.allocate(name, class);
                if self.start_quest(choice.quest, &mut alloc) {
                    self.story_history.insert(choice.quest, (starts, reset_at));
                    if node.flags & (1 << 16) != 0 {
                        self.story_selection
                            .entry(node.id)
                            .or_default()
                            .insert(choice.quest);
                    }
                    self.queue_story_event(choice.quest, event, &mut alloc);
                    started.push(choice.quest);
                }
            }
            if started.len() == started_before && node.flags & 2 != 0 {
                self.notifications.push(format!(
                    "Story Manager node {} found no eligible child quest",
                    node.id
                ));
            }
            return Ok(());
        }
        let children = ordered_children(nodes, parent);
        let mut eligible = Vec::new();
        for child in children {
            let Some(child_node) = nodes.iter().find(|node| node.id == child) else {
                continue;
            };
            if self.story_conditions(allocator, child_node, subject)? {
                eligible.push(child);
            }
        }
        let random = node.flags & 1 != 0;
        if random && !eligible.is_empty() {
            let index = self.random_int(0, eligible.len() as i32 - 1) as usize;
            let selected = eligible.swap_remove(index);
            self.route_node(nodes, selected, event, subject, budget, started, allocator)?;
        } else {
            for child in eligible {
                if started.len().saturating_sub(started_before) >= budget {
                    break;
                }
                self.route_node(nodes, child, event, subject, budget, started, allocator)?;
            }
        }
        if started.len() == started_before && node.flags & 2 != 0 {
            self.notifications.push(format!(
                "Story Manager node {} found no eligible child quest",
                node.id
            ));
        }
        Ok(())
    }

    fn descendant_quests(
        &self,
        nodes: &[Node],
        root: GlobalFormId,
    ) -> Result<BTreeSet<GlobalFormId>, StoryError> {
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        let mut quests = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if !seen.insert(parent) {
                continue;
            }
            if nodes
                .iter()
                .any(|node| node.id == parent && node.tag == T(b"SMQN"))
            {
                quests.extend(
                    self.quest_choices(parent)?
                        .into_iter()
                        .map(|choice| choice.quest),
                );
            }
            pending.extend(ordered_children(nodes, parent));
        }
        Ok(quests)
    }

    fn story_conditions(
        &mut self,
        allocator: &mut StoryAllocator<'_>,
        node: &Node,
        subject: GlobalFormId,
    ) -> Result<bool, StoryError> {
        let target = node
            .conditions
            .first()
            .and_then(|condition| (condition.run_on == 2).then_some(condition.reference))
            .and_then(|raw| {
                self.order
                    .winner_ref(node.id)
                    .map(|r| self.order.resolve(r.file, raw))
            });
        let context = Context {
            owner: node.id,
            subject,
            target,
            quest: None,
            tempering_enchanted: None,
        };
        let result = match allocator {
            StoryAllocator::Vm(vm) => conditions::evaluate(self, vm, context, &node.conditions),
            StoryAllocator::Native(_) => {
                conditions::evaluate_without_vm(self, context, &node.conditions)
            }
        };
        result.map_err(|error| StoryError::Condition(node.id, error.to_string()))
    }

    fn quest_choices(&self, node: GlobalFormId) -> Result<Vec<QuestChoice>, StoryError> {
        let rref = self
            .order
            .winner_ref(node)
            .ok_or(StoryError::Malformed(node))?;
        let rec = self.order.record(rref);
        let mut choices = Vec::new();
        let mut current = None;
        let mut twenty_four_hour_reset = false;
        let mut hours = 0.0;
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"NNAM" if sub.data.len() >= 4 => {
                    if let Some(quest) = current.take() {
                        choices.push(QuestChoice {
                            quest,
                            hours: if twenty_four_hour_reset { 24.0 } else { hours },
                        });
                    }
                    current = Some(
                        self.order.resolve(
                            rref.file,
                            u32::from_le_bytes(
                                sub.data[..4]
                                    .try_into()
                                    .map_err(|_| StoryError::Malformed(node))?,
                            ),
                        ),
                    );
                    twenty_four_hour_reset = false;
                    hours = 0.0;
                }
                b"FNAM" if !sub.data.is_empty() => twenty_four_hour_reset = sub.data[0] != 0,
                b"RNAM" if sub.data.len() >= 4 => {
                    hours = f64::from(f32::from_le_bytes(
                        sub.data[..4]
                            .try_into()
                            .map_err(|_| StoryError::Malformed(node))?,
                    ));
                    if !hours.is_finite() || hours < 0.0 {
                        return Err(StoryError::Reset(node));
                    }
                }
                _ => {}
            }
        }
        if let Some(quest) = current {
            choices.push(QuestChoice {
                quest,
                hours: if twenty_four_hour_reset { 24.0 } else { hours },
            });
        }
        Ok(choices)
    }

    fn node_u32(&self, node: GlobalFormId, tag: Tag) -> Option<u32> {
        let rref = self.order.winner_ref(node)?;
        self.order
            .record(rref)
            .find(tag)
            .filter(|data| data.len() >= 4)
            .and_then(|data| data[..4].try_into().ok().map(u32::from_le_bytes))
    }

    fn queue_story_event(&mut self, quest: GlobalFormId, event: &StoryEvent, alloc: Alloc<'_>) {
        let name = story_event_name(event.code);
        let args: Vec<Value> = event
            .parameters
            .iter()
            .map(|parameter| match parameter {
                crate::quest::state::StoryParameter::Integer(value) => Value::Int(*value as i32),
                crate::quest::state::StoryParameter::Reference { reference, .. } => {
                    self.form_value(*reference, alloc)
                }
            })
            .collect();
        let object = self.object_of(super::Handle::Form(quest), alloc);
        self.pending.push(Pending::Event {
            object,
            name: name.into(),
            args,
        });
    }
}

fn ordered_children(nodes: &[Node], parent: GlobalFormId) -> Vec<GlobalFormId> {
    let mut children: Vec<_> = nodes
        .iter()
        .filter(|node| node.parent == Some(parent))
        .map(|node| (node.id, node.previous))
        .collect();
    let mut ordered = Vec::with_capacity(children.len());
    let mut previous = None;
    while !children.is_empty() {
        let index = children
            .iter()
            .position(|(_, after)| *after == previous)
            .unwrap_or(0);
        let (id, _) = children.remove(index);
        ordered.push(id);
        previous = Some(id);
    }
    ordered
}

fn story_event_name(code: [u8; 4]) -> &'static str {
    match &code {
        b"AFAV" => "OnStoryActivateActor",
        b"AIPL" => "OnStoryAddToPlayer",
        b"ARRT" => "OnStoryArrest",
        b"ASSU" => "OnStoryAssaultActor",
        b"BRIB" => "OnStoryBribeNPC",
        b"CAST" => "OnStoryCastMagic",
        b"CLOC" => "OnStoryChangeLocation",
        b"CRFT" => "OnStoryCraftItem",
        b"ADCR" => "OnStoryCrimeGold",
        b"CHRR" => "OnStoryRelationshipChange",
        b"CURE" => "OnStoryCure",
        b"ADIA" => "OnStoryDialogue",
        b"DEAD" => "OnStoryDiscoverDeadBody",
        b"ESJA" => "OnStoryEscapeJail",
        b"FLAT" => "OnStoryFlatterNPC",
        b"AHEL" => "OnStoryHello",
        b"LEVL" => "OnStoryIncreaseLevel",
        b"SKIL" => "OnStoryIncreaseSkill",
        b"INFC" => "OnStoryInfection",
        b"INTM" => "OnStoryIntimidateNPC",
        b"JAIL" => "OnStoryJail",
        b"KILL" => "OnStoryKillActor",
        b"NVPE" => "OnStoryNewVoicePower",
        b"LOCK" => "OnStoryPickLock",
        b"PFIN" => "OnStoryPayFine",
        b"PICK" => "OnStoryPickpocket",
        b"PRFV" => "OnStoryPlayerGetsFavor",
        b"REMP" => "OnStoryRemoveFromPlayer",
        b"STIJ" => "OnStoryServedTime",
        b"TRES" => "OnStoryTrespass",
        b"QSTR" => "OnStoryQuestStart",
        _ => "OnStoryEvent",
    }
}

pub(super) fn write_history(world: &World, writer: &mut Writer) {
    writer.len(world.story_history.len());
    for (quest, (starts, deadline)) in &world.story_history {
        write_form_id(writer, *quest);
        writer.u32(*starts);
        writer.f64(*deadline);
    }
    writer.len(world.story_selection.len());
    for (node, quests) in &world.story_selection {
        write_form_id(writer, *node);
        writer.len(quests.len());
        for quest in quests {
            write_form_id(writer, *quest);
        }
    }
}

pub(super) fn read_history(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let count = reader.len()?;
    let mut history = std::collections::BTreeMap::new();
    for _ in 0..count {
        let quest = read_form_id(reader)?;
        let starts = reader.u32()?;
        let deadline = reader.f64()?;
        if !deadline.is_finite()
            || starts == 0
            || world.tag_of(quest) != Some(T(b"QUST"))
            || history.insert(quest, (starts, deadline)).is_some()
        {
            return Err(VmError::State("invalid PRG-14 story history".into()));
        }
    }
    let selection_count = reader.len()?;
    let mut selection = std::collections::BTreeMap::new();
    for _ in 0..selection_count {
        let node = read_form_id(reader)?;
        let quest_count = reader.len()?;
        let mut quests = std::collections::BTreeSet::new();
        for _ in 0..quest_count {
            let quest = read_form_id(reader)?;
            if world.tag_of(quest) != Some(T(b"QUST")) || !quests.insert(quest) {
                return Err(VmError::State("invalid PRG-14 selected quest".into()));
            }
        }
        if world.tag_of(node) != Some(T(b"SMQN")) || selection.insert(node, quests).is_some() {
            return Err(VmError::State("invalid PRG-14 story selection".into()));
        }
    }
    world.story_history = history;
    world.story_selection = selection;
    Ok(())
}

pub(super) const SECTION: Section<World> = Section {
    name: "PRG-14",
    write: write_history,
    read: read_history,
};

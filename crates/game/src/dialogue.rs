// SPDX-License-Identifier: GPL-3.0-or-later

//! Record-backed conversations. Layouts and selection inputs follow xEdit
//! DIAL/INFO/DLBR/QUST, UESP VMAD and the shipped TopicInfo fragment contract.
//! The host acknowledges each response before end fragments or links advance.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::records::gameplay::{DialogueInfo, DialogueTopic, Response};
use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::{Value, VmError};

use crate::conditions::{self, Context};
use crate::vmad::{InfoFragment, Vmad};
use crate::{Game, World};

mod index;
mod state;
pub(crate) use index::Index;
pub(crate) use state::{read_dialogue, read_dialogue_repeats};

/// Conversation cursor. Records remain authoritative for text and links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub speaker: GlobalFormId,
    pub listener: GlobalFormId,
    pub topic: Option<GlobalFormId>,
    pub current_info: Option<GlobalFormId>,
    pub last_info: Option<GlobalFormId>,
    pub response_index: u32,
}

/// Dialogue state that must survive an engine save.
#[derive(Debug, Default)]
pub(crate) struct State {
    pub session: Option<Session>,
    pub said: BTreeMap<GlobalFormId, f64>,
    /// INFO records selected in the current do-all cycle, keyed by DIAL.
    pub repeated: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
}

const GOODBYE: u16 = 1;
const RANDOM: u16 = 1 << 1;
const SAY_ONCE: u16 = 1 << 2;
const RANDOM_END: u16 = 1 << 5;
const TOP_LEVEL: u32 = 1;
const BLOCKING: u32 = 2;
const EXCLUSIVE: u32 = 4;
const BRANCH_FLAGS: u32 = TOP_LEVEL | BLOCKING | EXCLUSIVE;

#[derive(Debug)]
struct Branch {
    quest: GlobalFormId,
    start: GlobalFormId,
    flags: u32,
    category: u32,
}

struct Candidates {
    topic: DialogueTopic,
    quest_priority: u8,
    infos: Vec<(GlobalFormId, DialogueInfo)>,
}

/// One available topic and the INFO selected by current conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub topic: GlobalFormId,
    pub info: GlobalFormId,
    pub prompt: Option<String>,
    pub quest_priority: u8,
    pub topic_priority: f32,
}

/// Authored response metadata for the host's subtitle, voice and idle paths.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub topic: GlobalFormId,
    pub info: GlobalFormId,
    pub response_info: GlobalFormId,
    pub speaker: GlobalFormId,
    pub listener: GlobalFormId,
    pub response: Response,
    pub text: Option<String>,
    pub sound: Option<GlobalFormId>,
    pub speaker_idle: Option<GlobalFormId>,
    pub listener_idle: Option<GlobalFormId>,
}

/// Selection reports unsupported candidates instead of treating them as eligible.
#[derive(Debug, Default)]
pub struct Choices {
    pub available: Vec<Choice>,
    pub rejected: Vec<(GlobalFormId, String)>,
}

type RootTopics = (Vec<GlobalFormId>, Vec<(GlobalFormId, String)>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("missing or wrong dialogue record {0:?}")]
    Record(GlobalFormId),
    #[error("invalid dialogue record: {0}")]
    Data(#[from] soe_esm::Error),
    #[error("dialogue condition: {0}")]
    Condition(#[from] conditions::Error),
    #[error("dialogue script: {0}")]
    Script(#[from] VmError),
    #[error("dialogue unavailable: {0}")]
    State(&'static str),
    #[error("unsupported dialogue behavior: {0}")]
    Unsupported(&'static str),
    #[error("invalid dialogue ordering: {0}")]
    Ordering(String),
}

impl World {
    /// The owning DIAL comes from the winning INFO's TopicChildren group.
    pub fn dialogue_topic_of(&self, info: GlobalFormId) -> Option<GlobalFormId> {
        self.dialogue_index
            .get_or_init(|| Index::build(self.order()))
            .parents
            .get(&info)
            .copied()
    }

    /// Ordered INFO identities, retaining deleted entries as PNAM anchors.
    pub fn dialogue_infos(&self, topic: GlobalFormId) -> Result<Vec<GlobalFormId>, Error> {
        match self
            .dialogue_index
            .get_or_init(|| Index::build(self.order()))
            .topics
            .get(&topic)
        {
            Some(Ok(infos)) => Ok(infos.clone()),
            Some(Err(error)) => Err(Error::Ordering(error.clone())),
            None => Ok(Vec::new()),
        }
    }

    pub fn dialogue_topic(&self, id: GlobalFormId) -> Result<DialogueTopic, Error> {
        let rref = self.order().winner_ref(id).ok_or(Error::Record(id))?;
        Ok(DialogueTopic::from_record(
            self.order().record(rref),
            self.order().file(rref.file).plugin.header.is_localized(),
        )?)
    }

    pub fn dialogue_info(&self, id: GlobalFormId) -> Result<DialogueInfo, Error> {
        let rref = self.order().winner_ref(id).ok_or(Error::Record(id))?;
        if self.order().record(rref).is_deleted() {
            return Err(Error::Record(id));
        }
        Ok(DialogueInfo::from_record(
            self.order().record(rref),
            self.order().file(rref.file).plugin.header.is_localized(),
        )?)
    }

    /// Current player conversation, if one is open.
    pub fn dialogue_session(&self) -> Option<&Session> {
        self.dialogue.session.as_ref()
    }

    /// Actor.GetDialogueTarget returns None when called on the player.
    pub fn dialogue_target(&self, actor: GlobalFormId) -> Option<GlobalFormId> {
        let session = self.dialogue.session.as_ref()?;
        if self.is_player(actor) {
            return None;
        }
        if actor == session.speaker {
            Some(session.listener)
        } else if actor == session.listener {
            Some(session.speaker)
        } else {
            None
        }
    }

    /// Resolve shared response data without transferring conditions or fragments.
    fn response_data(&self, info: GlobalFormId) -> Result<(GlobalFormId, DialogueInfo), Error> {
        let mut current = info;
        let mut visited = std::collections::BTreeSet::new();
        loop {
            if !visited.insert(current) {
                return Err(Error::State("cyclic shared INFO"));
            }
            let data = self.dialogue_info(current)?;
            match data.shared_info.filter(|raw| *raw != 0) {
                Some(raw) => {
                    current = self
                        .resolve_from(current, raw)
                        .ok_or(Error::Record(current))?;
                }
                None => return Ok((current, data)),
            }
        }
    }

    fn dialogue_branch(&self, id: GlobalFormId) -> Result<Branch, Error> {
        let record = self
            .record(id)
            .filter(|r| r.tag == Tag(*b"DLBR") && !r.is_deleted())
            .ok_or(Error::Record(id))?;
        let field = |tag| -> Result<u32, Error> {
            let data = record.find(Tag(tag)).ok_or(Error::Record(id))?;
            Ok(u32::from_le_bytes(
                data.try_into().map_err(|_| Error::Record(id))?,
            ))
        };
        let quest = self
            .resolve_from(id, field(*b"QNAM")?)
            .ok_or(Error::Record(id))?;
        let start = self
            .resolve_from(id, field(*b"SNAM")?)
            .ok_or(Error::Record(id))?;
        Ok(Branch {
            quest,
            start,
            flags: field(*b"DNAM")?,
            category: field(*b"TNAM")?,
        })
    }

    fn topic_branch(&self, id: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        let topic = self.dialogue_topic(id)?;
        let branch = topic
            .branch
            .filter(|raw| *raw != 0)
            .map(|raw| self.resolve_from(id, raw).ok_or(Error::Record(id)))
            .transpose()?;
        Ok(branch.filter(|branch| {
            self.record(*branch)
                .is_some_and(|record| record.tag == Tag(*b"DLBR") && !record.is_deleted())
        }))
    }
}

impl Game {
    /// Open a conversation without inventing an eligible topic.
    /// The host queries choices and acknowledges actual response completion.
    ///
    /// # Errors
    ///
    /// Rejects a busy conversation or unavailable speaker/listener.
    pub fn begin_dialogue(
        &mut self,
        speaker: GlobalFormId,
        listener: GlobalFormId,
    ) -> Result<(), Error> {
        self.bind();
        let mut world = self.world.borrow_mut();
        if world.dialogue.session.is_some() {
            return Err(Error::State("conversation already open"));
        }
        if speaker == listener
            || world
                .actor(speaker)
                .is_none_or(|a| a.dead || a.in_combat || a.unconscious || !a.allow_pc_dialogue)
            || world.actor(listener).is_none_or(|a| a.dead)
        {
            return Err(Error::State("speaker or listener unavailable"));
        }
        world.dialogue.session = Some(Session {
            speaker,
            listener,
            topic: None,
            current_info: None,
            last_info: None,
            response_index: 0,
        });
        Ok(())
    }

    /// Close a conversation. Interrupted lines do not run their end fragments.
    pub fn close_dialogue(&mut self) {
        self.world.borrow_mut().dialogue.session = None;
    }

    /// Enumerate currently eligible player topics without advancing the RNG.
    ///
    /// # Errors
    ///
    /// Requires an open conversation between responses.
    pub fn dialogue_choices(&mut self) -> Result<Choices, Error> {
        let session = self
            .world
            .borrow()
            .dialogue
            .session
            .clone()
            .ok_or(Error::State("no conversation"))?;
        if session.current_info.is_some() {
            return Err(Error::State("response still playing"));
        }
        let linked_topics = if let Some(last) = session.last_info {
            let world = self.world.borrow();
            let info = world.dialogue_info(last)?;
            (!info.topic_links.is_empty()).then(|| {
                info.topic_links
                    .iter()
                    .filter_map(|raw| world.resolve_from(last, *raw))
                    .collect()
            })
        } else {
            None
        };
        let (candidates, rejected) = match linked_topics {
            Some(topics) => (topics, Vec::new()),
            None => self.root_dialogue_topics(&session),
        };
        let mut out = Choices {
            available: Vec::new(),
            rejected,
        };
        for topic in candidates {
            match self.dialogue_candidates(topic, &session) {
                Ok(Candidates {
                    topic: topic_data,
                    quest_priority,
                    infos,
                }) => {
                    if let Some((info, data)) = infos.first() {
                        let prompt = match (&data.prompt, &topic_data.name) {
                            (Some(text), _) => {
                                Some(self.world.borrow().resolve_text(*info, Tag(*b"RNAM"), text))
                            }
                            (None, Some(text)) => {
                                Some(self.world.borrow().resolve_text(topic, Tag(*b"FULL"), text))
                            }
                            (None, None) => None,
                        };
                        out.available.push(Choice {
                            topic,
                            info: *info,
                            prompt,
                            quest_priority,
                            topic_priority: topic_data.priority.unwrap_or(50.0),
                        });
                    }
                }
                Err(error) => out.rejected.push((topic, error.to_string())),
            }
        }
        out.available.sort_by(|a, b| {
            b.quest_priority
                .cmp(&a.quest_priority)
                .then_with(|| b.topic_priority.total_cmp(&a.topic_priority))
                .then(a.topic.cmp(&b.topic))
        });
        Ok(out)
    }

    fn root_dialogue_topics(&self, session: &Session) -> RootTopics {
        let active_exclusive = {
            let world = self.world.borrow();
            world
                .actor_ref(session.speaker)
                .and_then(|actor| actor.exclusive_branch)
        };
        let branches: Vec<_> = {
            let world = self.world.borrow();
            world
                .order()
                .winners_of(Tag(*b"DLBR"))
                .filter(|(_, record)| !record.is_deleted())
                .map(|(id, _)| (id, world.dialogue_branch(id)))
                .collect()
        };
        let mut topics = Vec::new();
        let mut blocking = Vec::new();
        let mut rejected = Vec::new();
        if let Some(id) = active_exclusive {
            let active_branch = { self.world.borrow().dialogue_branch(id) };
            match active_branch {
                Ok(branch) if branch.flags & !BRANCH_FLAGS != 0 => rejected.push((
                    id,
                    Error::Unsupported("unknown dialogue branch flags").to_string(),
                )),
                Ok(branch) if branch.flags & EXCLUSIVE == 0 => rejected.push((
                    id,
                    Error::State("active non-exclusive dialogue branch").to_string(),
                )),
                Ok(branch)
                    if branch.category != 0
                        || self.world.borrow().owning_quest(branch.start) != Some(branch.quest) =>
                {
                    rejected.push((
                        id,
                        Error::State("invalid active dialogue branch").to_string(),
                    ));
                }
                Ok(branch) => match self.dialogue_candidates(branch.start, session) {
                    Ok(candidates) if !candidates.infos.is_empty() => {
                        blocking.push((candidates.quest_priority, id, branch.start));
                    }
                    Ok(_) => {}
                    Err(error) => rejected.push((id, error.to_string())),
                },
                Err(error) => rejected.push((id, error.to_string())),
            }
        }
        for (id, branch) in branches {
            let branch = match branch {
                Ok(branch) => branch,
                Err(error) => {
                    rejected.push((id, error.to_string()));
                    continue;
                }
            };
            if branch.flags & !BRANCH_FLAGS != 0 {
                rejected.push((
                    id,
                    Error::Unsupported("unknown dialogue branch flags").to_string(),
                ));
                continue;
            }
            if branch.category != 0
                || branch.flags & (TOP_LEVEL | BLOCKING) == 0
                || active_exclusive == Some(id)
                || self.world.borrow().owning_quest(branch.start) != Some(branch.quest)
            {
                continue;
            }
            match self.dialogue_candidates(branch.start, session) {
                Ok(candidates) if !candidates.infos.is_empty() => {
                    if branch.flags & BLOCKING != 0 {
                        blocking.push((candidates.quest_priority, id, branch.start));
                    } else {
                        topics.push(branch.start);
                    }
                }
                Ok(_) => {}
                Err(error) => rejected.push((id, error.to_string())),
            }
        }
        if !blocking.is_empty() {
            blocking.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            return (vec![blocking[0].2], rejected);
        }
        topics.sort();
        topics.dedup();
        (topics, rejected)
    }

    fn dialogue_candidates(
        &self,
        topic: GlobalFormId,
        session: &Session,
    ) -> Result<Candidates, Error> {
        let mut world = self.world.borrow_mut();
        let topic_data = world.dialogue_topic(topic)?;
        if world.record(topic).is_none_or(|r| r.is_deleted()) {
            return Err(Error::Record(topic));
        }
        let quest = world.owning_quest(topic).ok_or(Error::Record(topic))?;
        let state = world.quest_state(quest).ok_or(Error::Record(quest))?;
        if !state.running {
            return Ok(Candidates {
                topic: topic_data,
                quest_priority: state.def.priority,
                infos: Vec::new(),
            });
        }
        let priority = state.def.priority;
        let quest_conditions = state.def.dialogue_conditions.clone();
        let context = Context {
            owner: quest,
            subject: session.speaker,
            target: Some(session.listener),
            quest: Some(quest),
            tempering_enchanted: None,
        };
        if !conditions::evaluate(&mut world, &self.vm, context, &quest_conditions)? {
            return Ok(Candidates {
                topic: topic_data,
                quest_priority: priority,
                infos: Vec::new(),
            });
        }
        let mut candidates = Vec::new();
        let mut random_group = false;
        for id in world.dialogue_infos(topic)? {
            if world.record(id).is_none_or(|record| record.is_deleted()) {
                continue;
            }
            let info = world.dialogue_info(id)?;
            let flags = info.data.as_ref().map_or(0, |data| data.flags());
            if random_group && flags & RANDOM == 0 {
                break;
            }
            if Self::info_eligible(&mut world, &self.vm, session, id, &info, quest)? {
                if flags & RANDOM == 0 {
                    candidates.push((id, info));
                    break;
                }
                random_group = true;
                candidates.push((id, info));
            }
            if flags & RANDOM_END != 0 && random_group {
                break;
            }
        }
        if topic_data.do_all_before_repeating.unwrap_or(0) != 0 {
            let said = world.dialogue.repeated.get(&topic);
            let unsaid: Vec<_> = candidates
                .iter()
                .filter(|(id, _)| !said.is_some_and(|infos| infos.contains(id)))
                .cloned()
                .collect();
            if !unsaid.is_empty() {
                candidates = unsaid;
            }
        }
        Ok(Candidates {
            topic: topic_data,
            quest_priority: priority,
            infos: candidates,
        })
    }

    fn info_eligible(
        world: &mut World,
        vm: &soe_papyrus::Vm,
        session: &Session,
        id: GlobalFormId,
        info: &DialogueInfo,
        quest: GlobalFormId,
    ) -> Result<bool, Error> {
        let flags = info.data.as_ref().map_or(0, |data| data.flags());
        if flags & (1 << 7) != 0 {
            return Ok(false);
        }
        if let Some(speaker) = info.speaker.filter(|raw| *raw != 0) {
            if world.base_of(session.speaker) != world.resolve_from(id, speaker) {
                return Ok(false);
            }
        }
        if let Some(time) = world.dialogue.said.get(&id) {
            let reset = info.data.as_ref().map_or(0.0, |data| data.reset_hours());
            if flags & SAY_ONCE != 0 || (world.clock.game_days - time) * 24.0 < f64::from(reset) {
                return Ok(false);
            }
        }
        let context = Context {
            owner: id,
            subject: session.speaker,
            target: Some(session.listener),
            quest: Some(quest),
            tempering_enchanted: None,
        };
        if !conditions::evaluate(world, vm, context, &info.conditions)? {
            return Ok(false);
        }
        if flags & ((1 << 4) | (1 << 6) | (1 << 14)) != 0 {
            return Err(Error::Unsupported(
                "refusal, invisible continuation or favor spending",
            ));
        }
        Ok(true)
    }

    /// Choose an offered topic, re-evaluate its conditions, and start its INFO.
    ///
    /// # Errors
    ///
    /// Rejects stale choices, malformed response links and missing fragment code.
    pub fn choose_dialogue(&mut self, topic: GlobalFormId) -> Result<Option<Line>, Error> {
        let choices = self.dialogue_choices()?;
        if !choices.available.iter().any(|choice| choice.topic == topic) {
            return Err(Error::State("topic is not currently offered"));
        }
        let session = self
            .world
            .borrow()
            .dialogue
            .session
            .clone()
            .ok_or(Error::State("no conversation"))?;
        let mut infos = self.dialogue_candidates(topic, &session)?.infos;
        if infos.is_empty() {
            return Err(Error::State("no eligible INFO"));
        }
        let index = if infos.len() > 1 {
            self.world
                .borrow_mut()
                .random_int(0, infos.len() as i32 - 1) as usize
        } else {
            0
        };
        let (info_id, info) = infos.swap_remove(index);
        self.world.borrow().response_data(info_id)?;
        let fragments = info
            .vmad
            .as_ref()
            .map(|raw| Vmad::parse(raw, Tag(*b"INFO")))
            .transpose()?;
        let object = self.object_of_form(info_id);
        if let Some(fragments) = fragments
            .as_ref()
            .and_then(|vmad| vmad.info_fragments.as_ref())
        {
            for fragment in [&fragments.begin, &fragments.end].into_iter().flatten() {
                if !self
                    .vm
                    .has_script_method(object, &fragment.script, &fragment.function)
                {
                    return Err(Error::Script(VmError::UnknownFunction {
                        class: fragment.script.clone(),
                        name: fragment.function.clone(),
                    }));
                }
            }
        }
        {
            let mut world = self.world.borrow_mut();
            let branch = world.topic_branch(topic)?;
            let active = world
                .actor_ref(session.speaker)
                .and_then(|actor| actor.exclusive_branch);
            let selected_branch = match branch {
                Some(id) => {
                    let branch = world.dialogue_branch(id)?;
                    if branch.flags & !BRANCH_FLAGS != 0 {
                        return Err(Error::Unsupported("unknown dialogue branch flags"));
                    }
                    Some((id, branch))
                }
                None => None,
            };
            if let Some(actor) = world.actor(session.speaker) {
                match selected_branch {
                    Some((id, branch)) if branch.flags & EXCLUSIVE != 0 => {
                        actor.exclusive_branch = Some(id);
                    }
                    Some(_) if active.is_some() => actor.exclusive_branch = None,
                    _ => {}
                }
            }
            let mut session = session;
            session.topic = Some(topic);
            session.current_info = Some(info_id);
            session.response_index = 0;
            world.dialogue.session = Some(session);
            let time = world.clock.game_days;
            world.dialogue.said.insert(info_id, time);
            if world
                .dialogue_topic(topic)?
                .do_all_before_repeating
                .unwrap_or(0)
                != 0
            {
                let completed = world.dialogue.repeated.entry(topic).or_default();
                if infos.iter().all(|(id, _)| completed.contains(id)) {
                    completed.clear();
                }
                completed.insert(info_id);
            }
        }
        if let Some(fragment) = fragments
            .as_ref()
            .and_then(|vmad| vmad.info_fragments.as_ref())
            .and_then(|f| f.begin.as_ref())
        {
            self.dialogue_fragment(info_id, fragment)?;
        }
        self.run_until_idle(64);
        let line = self.dialogue_line()?;
        if line.is_none() {
            self.finish_dialogue_info()?;
        }
        Ok(line)
    }

    /// Current authored response. Reading it has no playback or script side effects.
    pub fn dialogue_line(&self) -> Result<Option<Line>, Error> {
        let world = self.world.borrow();
        let Some(session) = &world.dialogue.session else {
            return Ok(None);
        };
        let Some(info) = session.current_info else {
            return Ok(None);
        };
        let topic = session
            .topic
            .ok_or(Error::State("response without topic"))?;
        let (response_info, data) = world.response_data(info)?;
        let Some(response) = data.responses.get(session.response_index as usize).cloned() else {
            return Ok(None);
        };
        let resolve = |raw| {
            if raw == 0 {
                None
            } else {
                world.resolve_from(response_info, raw)
            }
        };
        let text = response
            .text
            .as_ref()
            .map(|text| world.resolve_text(response_info, Tag(*b"NAM1"), text));
        Ok(Some(Line {
            topic,
            info,
            response_info,
            speaker: session.speaker,
            listener: session.listener,
            sound: resolve(response.sound),
            speaker_idle: response.speaker_idle.and_then(resolve),
            listener_idle: response.listener_idle.and_then(resolve),
            response,
            text,
        }))
    }

    pub fn advance_dialogue(&mut self) -> Result<Option<Line>, Error> {
        let next = {
            let world = self.world.borrow();
            let session = world
                .dialogue
                .session
                .as_ref()
                .ok_or(Error::State("no conversation"))?;
            let info = session
                .current_info
                .ok_or(Error::State("no response playing"))?;
            let (_, data) = world.response_data(info)?;
            let next = session
                .response_index
                .checked_add(1)
                .ok_or(Error::State("response index overflow"))?;
            ((next as usize) < data.responses.len()).then_some(next)
        };
        if let Some(next) = next {
            if let Some(session) = &mut self.world.borrow_mut().dialogue.session {
                session.response_index = next;
            }
            self.dialogue_line()
        } else {
            self.finish_dialogue_info()?;
            Ok(None)
        }
    }

    fn finish_dialogue_info(&mut self) -> Result<(), Error> {
        let (info_id, info) = {
            let world = self.world.borrow();
            let info_id = world
                .dialogue
                .session
                .as_ref()
                .and_then(|session| session.current_info)
                .ok_or(Error::State("no response playing"))?;
            (info_id, world.dialogue_info(info_id)?)
        };
        let fragments = info
            .vmad
            .as_ref()
            .map(|raw| Vmad::parse(raw, Tag(*b"INFO")))
            .transpose()?;
        if let Some(fragment) = fragments
            .as_ref()
            .and_then(|vmad| vmad.info_fragments.as_ref())
            .and_then(|f| f.end.as_ref())
        {
            self.dialogue_fragment(info_id, fragment)?;
        }
        self.run_until_idle(64);
        let mut world = self.world.borrow_mut();
        if info
            .data
            .as_ref()
            .is_some_and(|data| data.flags() & GOODBYE != 0)
        {
            world.dialogue.session = None;
        } else if let Some(session) = &mut world.dialogue.session {
            session.current_info = None;
            session.last_info = Some(info_id);
            session.response_index = 0;
        }
        Ok(())
    }

    fn dialogue_fragment(
        &mut self,
        info: GlobalFormId,
        fragment: &InfoFragment,
    ) -> Result<(), Error> {
        let speaker = self
            .world
            .borrow()
            .dialogue
            .session
            .as_ref()
            .ok_or(Error::State("no conversation"))?
            .speaker;
        let speaker = Value::Object(self.object_of_form(speaker));
        let object = self.object_of_form(info);
        self.vm
            .call_script(object, &fragment.script, &fragment.function, &[speaker])?;
        Ok(())
    }
}

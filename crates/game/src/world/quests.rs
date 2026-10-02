// SPDX-License-Identifier: GPL-3.0-or-later

//! Quests: the run-time state built from a `QUST` record on first use,
//! start and stop, stages with their fragments, objectives and aliases.

use std::collections::{BTreeMap, BTreeSet};

use soe_commonlib_shim::events::{EventSink, QuestStartStopEvent};
use soe_esm::GlobalFormId;
use thiserror::Error;

use super::{Alloc, Handle, ObjectiveState, Pending, QuestState, World};
use crate::quest::{AliasFill, Quest};
use crate::{conditions, quest::alias_flags};
use soe_esm::{records::gameplay::Condition, Tag};

const MAX_ALIAS_SEARCH_CANDIDATES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AliasResolutionError {
    #[error("quest alias dependency cycle: {0:?}")]
    Cycle(Vec<(GlobalFormId, u32)>),
}

impl World {
    pub(crate) fn initial_quest(&self, id: GlobalFormId) -> Option<QuestState> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        if rec.is_deleted() {
            return None;
        }
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        let def = Quest::from_record(rec, localized).ok()?;
        let aliases = def
            .aliases
            .iter()
            .map(|a| {
                let target = if a.location { None } else { a.forced_ref };
                (a.id, target.map(|raw| self.order.resolve(rref.file, raw)))
            })
            .collect();
        let runtime = crate::quest::state::Runtime {
            other_flags: def.flags & !crate::quest::state::STATE_FLAGS,
            ..Default::default()
        };
        Some(QuestState {
            def,
            running: false,
            completed: false,
            failed: false,
            active: false,
            current: 0,
            highest: 0,
            done: BTreeSet::new(),
            objectives: BTreeMap::new(),
            aliases,
            journal: crate::quest::state::Journal::default(),
            runtime,
            lifecycle_task: None,
        })
    }

    pub fn quest_state(&mut self, id: GlobalFormId) -> Option<&mut QuestState> {
        if !self.quests.contains_key(&id) {
            let state = self.initial_quest(id)?;
            self.quests.insert(id, state);
        }
        self.quests.get_mut(&id)
    }

    pub fn quest(&self, id: GlobalFormId) -> Option<&QuestState> {
        self.quests.get(&id)
    }

    /// Whether a quest is between being enabled and starting to run.
    ///
    /// Quest start normally completes synchronously. A startup task restored
    /// from a save remains observable until the lifecycle scheduler resumes it.
    ///
    /// # Errors
    ///
    /// This query does not return errors.
    ///
    /// # Panics
    ///
    /// This query does not panic.
    pub fn quest_is_starting(&mut self, id: GlobalFormId) -> bool {
        self.quest_state(id).is_some_and(|quest| {
            matches!(
                quest.lifecycle_task,
                Some(crate::quest::state::LifecycleTask::Starting)
            )
        })
    }

    /// Whether a quest is between being disabled and finishing its shutdown.
    ///
    /// Quest stop normally completes synchronously. A shutdown task restored
    /// from a save remains observable until the lifecycle scheduler resumes it.
    ///
    /// # Errors
    ///
    /// This query does not return errors.
    ///
    /// # Panics
    ///
    /// This query does not panic.
    pub fn quest_is_stopping(&mut self, id: GlobalFormId) -> bool {
        self.quest_state(id).is_some_and(|quest| {
            matches!(
                quest.lifecycle_task,
                Some(crate::quest::state::LifecycleTask::Stopping)
            )
        })
    }

    pub(crate) fn lifecycle_tasks(
        &self,
    ) -> Vec<(GlobalFormId, crate::quest::state::LifecycleTask)> {
        let mut tasks: Vec<_> = self
            .quests
            .iter()
            .filter_map(|(id, quest)| quest.lifecycle_task.map(|task| (*id, task)))
            .collect();
        tasks.sort_unstable_by_key(|(id, _)| *id);
        tasks
    }

    /// Register an MIT shim listener for successful quest start and stop transitions.
    pub fn add_quest_start_stop_sink(&mut self, sink: Box<dyn EventSink<QuestStartStopEvent>>) {
        self.quest_start_stop_events.add_sink(sink);
    }

    fn send_quest_start_stop(&mut self, id: GlobalFormId, started: bool) {
        let Some(quest) = self.quest(id) else {
            return;
        };
        let event = QuestStartStopEvent {
            form_id: soe_commonlib_shim::form::FormId(self.runtime_id(id)),
            started,
            failed: quest.failed,
        };
        self.quest_start_stop_events.send(&event);
    }

    /// `Quest.Start`: true when the quest was not running. Delivers `OnInit`
    /// to the quest's scripts, then sets the start-up stage.
    pub fn start_quest(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        if self.quest_state(id).is_none_or(|quest| quest.running) {
            return false;
        }
        let original_aliases = self.quest_state(id).map(|quest| quest.aliases.clone());
        if !self.fill_reference_aliases(id) {
            if let (Some(quest), Some(aliases)) = (self.quests.get_mut(&id), original_aliases) {
                quest.aliases = aliases;
            }
            return false;
        }
        if !self.fill_specific_location_aliases(id) {
            if let (Some(quest), Some(aliases)) = (self.quests.get_mut(&id), original_aliases) {
                quest.aliases = aliases;
            }
            return false;
        }
        let Some(q) = self.quest_state(id) else {
            return false;
        };
        if q.running {
            return false;
        }
        q.running = true;
        q.active = true;
        q.lifecycle_task = None;
        let startup = q.def.startup_stage();
        let alias_ids: Vec<u32> = q.def.aliases.iter().map(|a| a.id).collect();
        let object = self.object_of(Handle::Form(id), alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnInit".into(),
            args: Vec::new(),
        });
        // Alias scripts get their OnInit when the quest starts. Only an
        // alias something is attached to has an object by now.
        for alias in alias_ids {
            if let Some(object) = self.object_id(Handle::Alias {
                quest: id,
                id: alias,
            }) {
                self.pending.push(Pending::Event {
                    object,
                    name: "OnInit".into(),
                    args: Vec::new(),
                });
            }
        }
        self.send_quest_start_stop(id, true);
        if let Some(stage) = startup {
            self.set_stage(id, stage, alloc);
        }
        true
    }

    /// Fill reference aliases with one unambiguous, condition-matching placed reference.
    ///
    /// The search is capped across winning `ACHR` and `REFR` records. If the
    /// cap is exceeded, the result is incomplete and no searched alias is filled.
    fn fill_reference_aliases(&mut self, id: GlobalFormId) -> bool {
        let Some(state) = self.quest_state(id) else {
            return false;
        };
        let aliases: Vec<_> = state.def.aliases.clone();
        let mut candidates: Vec<_> = [Tag::new(b"ACHR"), Tag::new(b"REFR")]
            .into_iter()
            .flat_map(|tag| self.order.winners_of(tag).map(|(form, _)| form))
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        if candidates.len() > MAX_ALIAS_SEARCH_CANDIDATES {
            return aliases
                .iter()
                .filter(|alias| {
                    !alias.location
                        && !alias.creates_reference
                        && !alias.match_conditions.is_empty()
                        && alias.forced_ref.is_none()
                })
                .all(|alias| alias.flags & alias_flags::OPTIONAL != 0);
        }

        let mut pending = Vec::new();
        for alias in aliases.iter().filter(|alias| {
            !alias.location
                && !alias.creates_reference
                && alias.forced_ref.is_none()
                && !alias.match_conditions.is_empty()
        }) {
            if alias.flags & alias_flags::CLOSEST != 0 {
                if alias.flags & alias_flags::OPTIONAL == 0 {
                    return false;
                }
                continue;
            }
            let matching: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|candidate| {
                    self.alias_candidate_allowed(alias.flags, *candidate)
                        && self.alias_conditions_match(id, *candidate, &alias.match_conditions)
                        && !aliases.iter().any(|other| {
                            other.id != alias.id
                                && (self.alias_ref(id, other.id) == Some(*candidate)
                                    || pending.contains(&(other.id, *candidate)))
                                && (alias.flags & alias_flags::ALLOW_REUSE_IN_QUEST == 0
                                    || other.flags & alias_flags::RESERVES_REFERENCE != 0
                                        && alias.flags & alias_flags::ALLOW_RESERVED == 0)
                        })
                })
                .collect();
            let [candidate] = matching.as_slice() else {
                if alias.flags & alias_flags::OPTIONAL == 0 {
                    return false;
                }
                continue;
            };
            pending.push((alias.id, *candidate));
        }
        if let Some(state) = self.quests.get_mut(&id) {
            for (alias, target) in pending {
                state.aliases.insert(alias, Some(target));
            }
        }
        true
    }

    /// Fill location aliases whose QUST record names a specific `LCTN` in `ALFL`.
    fn fill_specific_location_aliases(&mut self, id: GlobalFormId) -> bool {
        let Some(quest_ref) = self.order.winner_ref(id) else {
            return false;
        };
        let Some(state) = self.quest_state(id) else {
            return false;
        };
        let aliases: Vec<_> = state
            .def
            .aliases
            .iter()
            .filter(|alias| alias.location && alias.forced_location.is_some())
            .map(|alias| (alias.id, alias.flags, alias.forced_location))
            .collect();
        let mut resolved = Vec::with_capacity(aliases.len());
        for (alias, flags, raw) in aliases {
            let target = raw
                .filter(|raw| *raw != 0)
                .map(|raw| self.order.resolve(quest_ref.file, raw))
                .filter(|target| {
                    self.order.winner(*target).is_some_and(|record| {
                        record.tag == Tag::new(b"LCTN") && !record.is_deleted()
                    })
                });
            let Some(target) = target else {
                if flags & alias_flags::OPTIONAL == 0 {
                    return false;
                }
                resolved.push((alias, None));
                continue;
            };
            resolved.push((alias, Some(target)));
        }
        if let Some(state) = self.quests.get_mut(&id) {
            for (alias, target) in resolved {
                state.aliases.insert(alias, target);
            }
        }
        true
    }

    fn alias_candidate_allowed(&mut self, flags: u32, candidate: GlobalFormId) -> bool {
        let Some((deleted, destroyed, enabled, is_actor, cell)) =
            self.reference(candidate).map(|reference| {
                (
                    reference.deleted,
                    reference.destroyed,
                    reference.enabled,
                    reference.is_actor,
                    reference.cell,
                )
            })
        else {
            return false;
        };
        if deleted || !enabled && flags & alias_flags::ALLOW_DISABLED == 0 {
            return false;
        }
        if destroyed && flags & alias_flags::ALLOW_DESTROYED == 0 {
            return false;
        }
        if flags & alias_flags::IN_LOADED_AREA != 0
            && !cell.is_some_and(|cell| self.cell_is_resident(cell))
        {
            return false;
        }
        if flags & alias_flags::ALLOW_DEAD == 0
            && is_actor
            && self.actor(candidate).is_some_and(|actor| actor.dead)
        {
            return false;
        }
        true
    }

    fn alias_conditions_match(
        &mut self,
        quest: GlobalFormId,
        candidate: GlobalFormId,
        conditions_list: &[Condition],
    ) -> bool {
        conditions::evaluate_without_vm(
            self,
            conditions::Context {
                owner: quest,
                subject: candidate,
                target: None,
                quest: Some(quest),
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
                skill_use: None,
                light_level: None,
            },
            conditions_list,
        )
        .unwrap_or(false)
    }

    pub fn stop_quest(&mut self, id: GlobalFormId) -> bool {
        let stopped = match self.quest_state(id) {
            Some(q) if q.running => {
                q.running = false;
                q.active = false;
                q.lifecycle_task = None;
                true
            }
            _ => false,
        };
        if stopped {
            self.send_quest_start_stop(id, false);
        }
        stopped
    }

    /// `Quest.SetStage`: starts the quest when needed, records the stage,
    /// applies the log entry flags, and queues the stage fragment. Returns
    /// false for a stage the quest does not define.
    pub fn set_stage(&mut self, id: GlobalFormId, stage: u16, alloc: Alloc<'_>) -> bool {
        let Some(q) = self.quest_state(id) else {
            return false;
        };
        if q.def.stage(stage).is_none() {
            return false;
        }
        let was_running = q.running;
        let started = !q.running && !q.completed;
        if started {
            q.running = true;
            q.active = true;
            let object = self.object_of(Handle::Form(id), alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnInit".into(),
                args: Vec::new(),
            });
        }
        let Some(q) = self.quests.get_mut(&id) else {
            return false;
        };
        q.current = stage;
        q.highest = q.highest.max(stage);
        q.done.insert(stage);
        let Some(def_stage) = q.def.stage(stage) else {
            return false;
        };
        if def_stage.completes_quest() {
            q.completed = true;
            q.running = false;
        }
        if def_stage.fails_quest() {
            q.failed = true;
            q.running = false;
        }
        let is_running = q.running;
        let fragments: Vec<String> = q
            .def
            .vmad
            .as_ref()
            .map(|v| {
                v.fragments
                    .iter()
                    .filter(|f| f.stage == stage)
                    .map(|f| f.function.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !fragments.is_empty() {
            let object = self.object_of(Handle::Form(id), alloc);
            for function in fragments {
                self.pending.push(Pending::Call {
                    object,
                    function,
                    args: Vec::new(),
                });
            }
        }
        if started {
            self.send_quest_start_stop(id, true);
        }
        if (was_running || started) && !is_running {
            self.send_quest_start_stop(id, false);
        }
        self.commonlib_quest_stage(id, stage);
        true
    }

    pub fn objective(&mut self, id: GlobalFormId, index: u16) -> Option<&mut ObjectiveState> {
        let q = self.quest_state(id)?;
        q.def.objective(index)?;
        Some(q.objectives.entry(index).or_default())
    }

    pub fn alias_ref(&mut self, quest: GlobalFormId, alias: u32) -> Option<GlobalFormId> {
        self.resolve_alias_ref(quest, alias).ok().flatten()
    }

    /// Resolve an alias through its local or external alias fill dependency.
    /// No alias state is written while walking the dependency chain.
    pub fn resolve_alias_ref(
        &mut self,
        quest: GlobalFormId,
        alias: u32,
    ) -> Result<Option<GlobalFormId>, AliasResolutionError> {
        self.resolve_alias_ref_inner(quest, alias, &mut Vec::new())
    }

    fn resolve_alias_ref_inner(
        &mut self,
        quest: GlobalFormId,
        alias: u32,
        path: &mut Vec<(GlobalFormId, u32)>,
    ) -> Result<Option<GlobalFormId>, AliasResolutionError> {
        let node = (quest, alias);
        if let Some(start) = path.iter().position(|entry| *entry == node) {
            let mut cycle = path[start..].to_vec();
            cycle.push(node);
            return Err(AliasResolutionError::Cycle(cycle));
        }
        let Some((fill, current_target)) = self.quest_state(quest).and_then(|state| {
            state.def.alias(alias).map(|definition| {
                (
                    definition.fill,
                    state.aliases.get(&alias).copied().flatten(),
                )
            })
        }) else {
            return Ok(None);
        };
        if let Some(target) = current_target {
            return Ok(Some(target));
        }
        path.push(node);
        let result = match fill {
            Some(AliasFill::Local(target)) => self.resolve_alias_ref_inner(quest, target, path)?,
            Some(AliasFill::External {
                quest: raw_quest,
                alias: target,
            }) => {
                let Some(rref) = self.order.winner_ref(quest) else {
                    path.pop();
                    return Ok(None);
                };
                let target_quest = self.order.resolve(rref.file, raw_quest);
                self.resolve_alias_ref_inner(target_quest, target, path)?
            }
            None => current_target,
        };
        path.pop();
        Ok(result)
    }

    /// Whether this quest declares `alias` as an `ALLS` location alias.
    pub fn alias_is_location(&mut self, quest: GlobalFormId, alias: u32) -> bool {
        self.quest_state(quest)
            .and_then(|state| state.def.alias(alias))
            .is_some_and(|definition| definition.location)
    }

    pub fn force_alias(
        &mut self,
        quest: GlobalFormId,
        alias: u32,
        target: Option<GlobalFormId>,
    ) -> bool {
        match self.quest_state(quest) {
            Some(q) if q.def.alias(alias).is_some() => {
                q.aliases.insert(alias, target);
                true
            }
            _ => false,
        }
    }
}

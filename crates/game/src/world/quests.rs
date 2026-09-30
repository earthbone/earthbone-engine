// SPDX-License-Identifier: GPL-3.0-or-later

//! Quests: the run-time state built from a `QUST` record on first use,
//! start and stop, stages with their fragments, objectives and aliases.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use thiserror::Error;

use super::{Alloc, Handle, ObjectiveState, Pending, QuestState, World};
use crate::quest::{AliasFill, Quest};

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
            .map(|a| (a.id, a.forced_ref.map(|r| self.order.resolve(rref.file, r))))
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

    /// `Quest.Start`: true when the quest was not running. Delivers `OnInit`
    /// to the quest's scripts, then sets the start-up stage.
    pub fn start_quest(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
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
        if let Some(stage) = startup {
            self.set_stage(id, stage, alloc);
        }
        true
    }

    pub fn stop_quest(&mut self, id: GlobalFormId) -> bool {
        match self.quest_state(id) {
            Some(q) if q.running => {
                q.running = false;
                q.active = false;
                q.lifecycle_task = None;
                true
            }
            _ => false,
        }
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
        if !q.running && !q.completed {
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

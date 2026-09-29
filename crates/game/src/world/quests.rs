// SPDX-License-Identifier: GPL-3.0-or-later

//! Quests: the run-time state built from a `QUST` record on first use,
//! start and stop, stages with their fragments, objectives and aliases.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;

use super::{Alloc, Handle, ObjectiveState, Pending, QuestState, World};
use crate::quest::Quest;

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
    /// Quest start currently completes synchronously. Transitional state from
    /// a restored save is not represented until SAV-10 adds resumed lifecycle
    /// tasks, so this returns false for every quest in the current model.
    ///
    /// # Errors
    ///
    /// This query does not return errors.
    ///
    /// # Panics
    ///
    /// This query does not panic.
    pub fn quest_is_starting(&mut self, id: GlobalFormId) -> bool {
        let _state = self.quest_state(id);
        false
    }

    /// Whether a quest is between being disabled and finishing its shutdown.
    ///
    /// Quest stop currently completes synchronously. Transitional state from
    /// a restored save is not represented until SAV-10 adds resumed lifecycle
    /// tasks, so this returns false for every quest in the current model.
    ///
    /// # Errors
    ///
    /// This query does not return errors.
    ///
    /// # Panics
    ///
    /// This query does not panic.
    pub fn quest_is_stopping(&mut self, id: GlobalFormId) -> bool {
        let _state = self.quest_state(id);
        false
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
        self.quest_state(quest)?
            .aliases
            .get(&alias)
            .copied()
            .flatten()
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

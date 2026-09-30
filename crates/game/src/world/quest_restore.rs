// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply QUST state without executing quest commands. Layout follows xEdit
//! and UESP. Objective states and journal fields follow CommonLib TESQuest.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_save::{quest as saved, RefId, Save};

use super::{ObjectiveState, QuestState, World};
use crate::quest::state::{
    AliasMetadata, Journal, JournalInstance, RunData, StoryEvent, StoryParameter, STATE_FLAGS,
};
use crate::vanilla_save::{RestoreError, SavedFormMap};

/// Applied quests and identities requiring later world/execution restoration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestRestoreReport {
    pub quests: usize,
    pub unresolved_forms: BTreeSet<GlobalFormId>,
    /// Quests whose saved stage-wait bit requires scheduler restoration.
    pub stage_wait: BTreeSet<GlobalFormId>,
}

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Quest {
        id,
        reason: reason.into(),
    }
}

struct Context<'a, 's> {
    world: &'a World,
    mapping: SavedFormMap<'s>,
    report: QuestRestoreReport,
}

impl Context<'_, '_> {
    fn reference(&mut self, value: RefId) -> Result<Option<GlobalFormId>, RestoreError> {
        let target = self.mapping.reference(value)?;
        if let Some(id) = target {
            if self
                .world
                .order
                .winner(id)
                .is_none_or(|record| record.is_deleted())
                && !self.world.refs.contains_key(&id)
            {
                self.report.unresolved_forms.insert(id);
            }
        }
        Ok(target)
    }

    fn alias(
        &self,
        id: GlobalFormId,
        q: &QuestState,
        alias: u32,
        location: bool,
    ) -> Result<(), RestoreError> {
        if q.def
            .alias(alias)
            .is_none_or(|def| def.location != location)
        {
            return Err(invalid(id, format!("unknown or wrong-kind alias {alias}")));
        }
        Ok(())
    }

    fn event(&mut self, event: saved::StoryEvent) -> Result<StoryEvent, RestoreError> {
        Ok(StoryEvent {
            id: event.id,
            code: event.code,
            parameters: event
                .parameters
                .into_iter()
                .map(|parameter| {
                    Ok(match parameter {
                        saved::EventParameter::Integer(value) => StoryParameter::Integer(value),
                        saved::EventParameter::Reference { kind, reference } => {
                            StoryParameter::Reference {
                                kind,
                                reference: self.reference(reference)?,
                            }
                        }
                    })
                })
                .collect::<Result<_, RestoreError>>()?,
        })
    }

    fn runtime(
        &mut self,
        id: GlobalFormId,
        q: &mut QuestState,
        run: saved::Runtime,
    ) -> Result<(), RestoreError> {
        let mut metadata = BTreeMap::new();
        let mut seen = BTreeSet::new();
        // CONTEXT: A present runtime section replaces alias fill results,
        // including empty slots that differ from authored forced references.
        q.aliases.values_mut().for_each(|value| *value = None);
        for alias in run.aliases {
            self.alias(id, q, alias.id, false)?;
            if !seen.insert(alias.id) {
                return Err(invalid(id, format!("duplicate runtime alias {}", alias.id)));
            }
            let mut references = alias.references.into_iter();
            let first = references
                .next()
                .ok_or_else(|| invalid(id, "alias has no target"))?;
            q.aliases.insert(alias.id, self.reference(first)?);
            metadata.insert(
                alias.id,
                AliasMetadata {
                    kind: alias.kind,
                    references: references
                        .map(|value| self.reference(value))
                        .collect::<Result<_, _>>()?,
                },
            );
        }
        for alias in run.locations {
            self.alias(id, q, alias.id, true)?;
            if !seen.insert(alias.id) {
                return Err(invalid(id, format!("duplicate runtime alias {}", alias.id)));
            }
            q.aliases.insert(alias.id, self.reference(alias.reference)?);
        }
        q.runtime.run_data = Some(RunData {
            unknown: run.unknown,
            aliases: metadata,
            event_flag: run.event_flag,
            event: run.event.map(|event| self.event(event)).transpose()?,
        });
        Ok(())
    }

    fn instances(
        &mut self,
        id: GlobalFormId,
        q: &mut QuestState,
        instances: saved::Instances,
    ) -> Result<(), RestoreError> {
        let mut journal = Journal {
            current: instances.unknown,
            ..Journal::default()
        };
        for instance in instances.instances.entries {
            let mut entry = JournalInstance {
                stage: instance.unknown_short,
                stage_item: instance.unknown_byte as i8,
                ..JournalInstance::default()
            };
            for alias in instance.aliases.entries {
                if q.def.alias(alias.id).is_none() {
                    return Err(invalid(id, format!("unknown journal alias {}", alias.id)));
                }
                if entry
                    .aliases
                    .insert(alias.id, self.reference(alias.reference)?)
                    .is_some()
                {
                    return Err(invalid(id, format!("duplicate journal alias {}", alias.id)));
                }
            }
            for global in instance.globals.entries {
                let target = self
                    .reference(global.form)?
                    .ok_or_else(|| invalid(id, "null journal global"))?;
                if !self.world.order.winner(target).is_some_and(|record| {
                    record.tag == soe_esm::Tag(*b"GLOB") && !record.is_deleted()
                }) {
                    return Err(invalid(
                        id,
                        format!("journal global {target} is not a GLOB"),
                    ));
                }
                if entry.globals.insert(target, global.value).is_some() {
                    return Err(invalid(id, format!("duplicate journal global {target}")));
                }
            }
            if journal.instances.insert(instance.unknown, entry).is_some() {
                return Err(invalid(
                    id,
                    format!("duplicate journal instance {}", instance.unknown),
                ));
            }
        }
        if !journal.instances.is_empty() && !journal.instances.contains_key(&journal.current) {
            return Err(invalid(id, "current journal instance is absent"));
        }
        q.journal = journal;
        Ok(())
    }

    fn quest(
        &mut self,
        id: GlobalFormId,
        change: saved::QuestChange,
    ) -> Result<QuestState, RestoreError> {
        let mut q = self
            .world
            .initial_quest(id)
            .ok_or_else(|| invalid(id, "missing, deleted or malformed QUST"))?;
        if let Some(flags) = change.quest_flags {
            q.running = flags & 1 != 0;
            q.completed = flags & 2 != 0;
            q.failed = flags & 0x40 != 0;
            q.active = flags & 0x800 != 0;
            q.runtime.other_flags = flags & !STATE_FLAGS;
            q.lifecycle_task = if flags & !1 == !1 {
                Some(if flags & 1 != 0 {
                    crate::quest::state::LifecycleTask::Starting
                } else {
                    crate::quest::state::LifecycleTask::Stopping
                })
            } else {
                None
            };
            match q.lifecycle_task {
                Some(crate::quest::state::LifecycleTask::Starting) => q.running = false,
                Some(crate::quest::state::LifecycleTask::Stopping) => q.running = true,
                None => {}
            }
            if flags & 0x80 != 0 {
                self.report.stage_wait.insert(id);
            }
        }
        q.runtime.form_flags = change.form_flags.map(|flags| (flags.flags, flags.unknown));
        q.runtime.script_delay = change.script_delay;
        q.runtime.already_run = change.already_run.unwrap_or(0);
        if let Some(stages) = change.stages {
            let mut seen = BTreeSet::new();
            for stage in stages.entries {
                let index = stage.stage as u16;
                if stage.status > 1 || q.def.stage(index).is_none() || !seen.insert(index) {
                    return Err(invalid(id, format!("invalid or duplicate stage {index}")));
                }
                if stage.status == 1 {
                    q.done.insert(index);
                }
            }
            q.highest = q.done.last().copied().unwrap_or(0);
            q.current = q.highest;
        }
        if let Some(objectives) = change.objectives {
            for objective in objectives.entries {
                let index = u16::try_from(objective.id)
                    .map_err(|_| invalid(id, "objective ID exceeds u16"))?;
                if objective.status > 5 || q.def.objective(index).is_none() {
                    return Err(invalid(
                        id,
                        format!("invalid objective {index} status {}", objective.status),
                    ));
                }
                let state = ObjectiveState {
                    displayed: objective.status & 1 != 0,
                    completed: matches!(objective.status, 2 | 3),
                    failed: matches!(objective.status, 4 | 5),
                };
                if q.objectives.insert(index, state).is_some() {
                    return Err(invalid(id, format!("duplicate objective {index}")));
                }
            }
        }
        if let Some(runtime) = change.runtime {
            self.runtime(id, &mut q, runtime)?;
        }
        if let Some(instances) = change.instances {
            self.instances(id, &mut q, instances)?;
        }
        Ok(q)
    }
}

impl World {
    pub(crate) fn saved_story_events(&self) -> Vec<(GlobalFormId, StoryEvent)> {
        self.quests
            .iter()
            .filter_map(|(id, quest)| {
                quest
                    .runtime
                    .run_data
                    .as_ref()
                    .and_then(|run| run.event.as_ref())
                    .map(|event| (*id, event.clone()))
            })
            .collect()
    }

    pub(crate) fn clear_saved_story_event(&mut self, id: GlobalFormId) {
        if let Some(run) = self
            .quests
            .get_mut(&id)
            .and_then(|quest| quest.runtime.run_data.as_mut())
        {
            run.event = None;
        }
    }

    /// Apply all saved QUST change forms after validating the entire batch.
    /// Unlisted quests remain unchanged. No startup, alias-fill, fragment or
    /// story event is dispatched. The report names missing world identities.
    ///
    /// # Errors
    /// Invalid plugin/reference mapping, malformed quest payloads, missing
    /// definitions, invalid stage/objective/alias IDs or repeated entries.
    pub fn restore_vanilla_quests(
        &mut self,
        save: &Save,
    ) -> Result<QuestRestoreReport, RestoreError> {
        let mut context = Context {
            world: self,
            mapping: SavedFormMap::new(save, &self.order)?,
            report: QuestRestoreReport::default(),
        };
        let mut quests = BTreeMap::new();
        for change in &save.change_forms {
            let Some(data) = change.quest_change()? else {
                continue;
            };
            let id = context
                .mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            if quests.contains_key(&id) {
                return Err(invalid(id, "duplicate QUST change form"));
            }
            quests.insert(id, context.quest(id, data)?);
        }
        context.report.quests = quests.len();
        let report = context.report;
        self.quests.extend(quests);
        Ok(report)
    }
}

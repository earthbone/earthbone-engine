// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW quest journal/runtime extension. The live structures use mapped
//! identities, with field meanings documented in quest::state.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use soe_esm::GlobalFormId;
use soe_papyrus::{vm::state::Writer, VmError};

use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{QuestState, World};
use crate::quest::state::{
    AliasMetadata, Journal, JournalInstance, RunData, Runtime, StoryEvent, StoryParameter,
    STATE_FLAGS,
};

fn bad(reason: &str) -> VmError {
    VmError::State(format!("quest state: {reason}"))
}

pub(super) fn write_aux(w: &mut Writer, q: &QuestState) {
    let rt = &q.runtime;
    w.u16(rt.other_flags);
    w.bool(rt.form_flags.is_some());
    if let Some((flags, unknown)) = rt.form_flags {
        w.u32(flags);
        w.u16(unknown);
    }
    w.bool(rt.script_delay.is_some());
    if let Some(delay) = rt.script_delay {
        w.f32(delay);
    }
    w.u8(rt.already_run);
    w.bool(rt.run_data.is_some());
    if let Some(run) = &rt.run_data {
        w.u8(run.unknown);
        w.len(run.aliases.len());
        for (id, alias) in &run.aliases {
            w.u32(*id);
            w.u8(alias.kind);
            w.len(alias.references.len());
            for reference in &alias.references {
                write_opt_form_id(w, *reference);
            }
        }
        w.u8(run.event_flag);
        w.bool(run.event.is_some());
        if let Some(event) = &run.event {
            w.u32(event.id);
            w.out.extend_from_slice(&event.code);
            w.len(event.parameters.len());
            for parameter in &event.parameters {
                match parameter {
                    StoryParameter::Integer(value) => {
                        w.u32(3);
                        w.u32(*value);
                    }
                    StoryParameter::Reference { kind, reference } => {
                        w.u32(*kind);
                        write_opt_form_id(w, *reference);
                    }
                }
            }
        }
    }
    w.u32(q.journal.current);
    w.len(q.journal.instances.len());
    for (id, instance) in &q.journal.instances {
        w.u32(*id);
        w.u16(instance.stage);
        w.u8(instance.stage_item as u8);
        w.len(instance.aliases.len());
        for (alias, reference) in &instance.aliases {
            w.u32(*alias);
            write_opt_form_id(w, *reference);
        }
        w.len(instance.globals.len());
        for (global, value) in &instance.globals {
            write_form_id(w, *global);
            w.f32(*value);
        }
    }
}

fn read_run(r: &mut FormReader<'_, '_>, q: &QuestState) -> Result<RunData, VmError> {
    let unknown = r.u8()?;
    let n = r.len()?;
    let mut aliases = BTreeMap::new();
    for _ in 0..n {
        let id = r.u32()?;
        let kind = r.u8()?;
        let count = r.len()?;
        if q.def.alias(id).is_none_or(|alias| alias.location)
            || count != if kind == 0 { 0 } else { 4 }
        {
            return Err(bad("invalid reference alias metadata"));
        }
        let mut references = Vec::new();
        for _ in 0..count {
            references.push(read_opt_form_id(r)?);
        }
        if aliases
            .insert(id, AliasMetadata { kind, references })
            .is_some()
        {
            return Err(bad("duplicate reference alias metadata"));
        }
    }
    let event_flag = r.u8()?;
    let event = if r.bool()? {
        let id = r.u32()?;
        let mut code = [0; 4];
        code.copy_from_slice(r.take(4)?);
        let n = r.len()?;
        let mut parameters = Vec::new();
        for _ in 0..n {
            parameters.push(match r.u32()? {
                3 => StoryParameter::Integer(r.u32()?),
                kind @ (0 | 1 | 2 | 4) => StoryParameter::Reference {
                    kind,
                    reference: read_opt_form_id(r)?,
                },
                _ => return Err(bad("invalid story event parameter")),
            });
        }
        Some(StoryEvent {
            id,
            code,
            parameters,
        })
    } else {
        None
    };
    if (event_flag != 0) != event.is_some() {
        return Err(bad("story event presence mismatch"));
    }
    Ok(RunData {
        unknown,
        aliases,
        event_flag,
        event,
    })
}

fn read_aux(r: &mut FormReader<'_, '_>, q: &mut QuestState) -> Result<(), VmError> {
    let other_flags = r.u16()?;
    if other_flags & STATE_FLAGS != 0 {
        return Err(bad("duplicated quest state flags"));
    }
    let form_flags = if r.bool()? {
        Some((r.u32()?, r.u16()?))
    } else {
        None
    };
    let script_delay = if r.bool()? { Some(r.f32()?) } else { None };
    let already_run = r.u8()?;
    let run_data = if r.bool()? {
        Some(read_run(r, q)?)
    } else {
        None
    };
    let runtime = Runtime {
        other_flags,
        form_flags,
        script_delay,
        already_run,
        run_data,
    };
    let current = r.u32()?;
    let n = r.len()?;
    let mut instances = BTreeMap::new();
    for _ in 0..n {
        let id = r.u32()?;
        let stage = r.u16()?;
        let stage_item = r.u8()? as i8;
        let n = r.len()?;
        let mut aliases = BTreeMap::new();
        for _ in 0..n {
            let alias = r.u32()?;
            if q.def.alias(alias).is_none() {
                return Err(bad("unknown journal alias"));
            }
            if aliases.insert(alias, read_opt_form_id(r)?).is_some() {
                return Err(bad("duplicate journal alias"));
            }
        }
        let n = r.len()?;
        let mut globals = BTreeMap::new();
        for _ in 0..n {
            if globals.insert(read_form_id(r)?, r.f32()?).is_some() {
                return Err(bad("duplicate journal global"));
            }
        }
        if instances
            .insert(
                id,
                JournalInstance {
                    aliases,
                    globals,
                    stage,
                    stage_item,
                },
            )
            .is_some()
        {
            return Err(bad("duplicate journal instance"));
        }
    }
    if !instances.is_empty() && !instances.contains_key(&current) {
        return Err(bad("missing current journal instance"));
    }
    let journal = Journal { current, instances };
    let old = q
        .journal
        .globals()
        .into_iter()
        .flatten()
        .map(|(id, v)| (*id, v.to_bits()));
    let new = journal
        .globals()
        .into_iter()
        .flatten()
        .map(|(id, v)| (*id, v.to_bits()));
    if !old.eq(new) {
        return Err(bad("current globals differ from legacy table"));
    }
    q.runtime = runtime;
    q.journal = journal;
    Ok(())
}

impl World {
    pub(super) fn write_quest_aux(&self, w: &mut Writer) {
        let ordered: BTreeMap<_, _> = self.quests.iter().collect();
        w.len(ordered.len());
        for (id, q) in ordered {
            write_form_id(w, *id);
            write_aux(w, q);
        }
    }
}

pub(super) fn read_quest_aux(
    r: &mut FormReader<'_, '_>,
    quests: &mut HashMap<GlobalFormId, QuestState>,
) -> Result<(), VmError> {
    let n = r.len()?;
    if n != quests.len() {
        return Err(bad("quest extension count differs from quest table"));
    }
    let mut seen = BTreeSet::new();
    for _ in 0..n {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(bad("duplicate quest extension"));
        }
        let q = quests
            .get_mut(&id)
            .ok_or_else(|| bad("unknown quest extension"))?;
        read_aux(r, q)?;
    }
    Ok(())
}

pub(super) fn write_lifecycle_tasks(world: &World, w: &mut Writer) {
    let tasks: BTreeMap<_, _> = world
        .quests
        .iter()
        .filter_map(|(id, quest)| quest.lifecycle_task.map(|task| (*id, task)))
        .collect();
    if tasks.is_empty() {
        return;
    }
    w.len(tasks.len());
    for (id, task) in tasks {
        write_form_id(w, id);
        w.u8(match task {
            crate::quest::state::LifecycleTask::Starting => 1,
            crate::quest::state::LifecycleTask::Stopping => 2,
        });
    }
}

pub(super) fn read_lifecycle_tasks(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let mut seen = BTreeSet::new();
    for _ in 0..r.len()? {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(bad("duplicate quest lifecycle task"));
        }
        let task = match r.u8()? {
            1 => crate::quest::state::LifecycleTask::Starting,
            2 => crate::quest::state::LifecycleTask::Stopping,
            _ => return Err(bad("invalid quest lifecycle task")),
        };
        let quest = world
            .quests
            .get_mut(&id)
            .ok_or_else(|| bad("lifecycle task names an unknown quest"))?;
        quest.lifecycle_task = Some(task);
    }
    Ok(())
}

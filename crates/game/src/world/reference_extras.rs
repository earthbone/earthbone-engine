// SPDX-License-Identifier: GPL-3.0-or-later

//! Atomic reference ownership and lock restoration. Layouts follow UESP and
//! FallrimTools, with runtime lock flags from CommonLib REFR_LOCK. ExtraRank
//! uses xEdit's signed word. Unassigned metadata stays available without events.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use soe_esm::GlobalFormId;
use soe_papyrus::{vm::state::Writer, VmError};
use soe_save::{reference::Field, Save};

use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{RefState, World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

/// Lock fields outside the shared locked bit and level. An absent key here
/// overrides the authored key. The two words retain their saved bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReferenceLock {
    pub key: Option<GlobalFormId>,
    /// All saved flag bits except locked, which belongs to RefState::locked.
    pub flags: u8,
    pub unknown: [u32; 2],
}

/// Applied reference extras. Leveled lock scaling remains a separate mechanic.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReferenceExtrasRestoreReport {
    pub references: usize,
    pub owners: usize,
    pub locks: usize,
    pub leveled_locks: BTreeSet<GlobalFormId>,
}

fn bad(id: GlobalFormId, reason: impl std::fmt::Display) -> RestoreError {
    RestoreError::WorldReference {
        id,
        reason: reason.to_string(),
    }
}

impl World {
    /// Apply saved ownership, rank/global metadata and lock extras to resident
    /// references. Missing entries with the corresponding change bit clear
    /// ownership or the lock. Other fields and unlisted references are unchanged.
    /// Preparation validates every target and link before any live mutation.
    /// No activation, lock, ownership or script events are dispatched.
    ///
    /// # Errors
    /// Invalid payloads or plugin mappings, duplicate targets/extras, absent
    /// reference placements, or owner/key/global links of the wrong record type.
    pub fn restore_vanilla_reference_extras(
        &mut self,
        save: &Save,
    ) -> Result<ReferenceExtrasRestoreReport, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        let mut report = ReferenceExtrasRestoreReport::default();
        for change in &save.change_forms {
            let Some(extra) = change.reference_extra_data(save.form_version)? else {
                continue;
            };
            let owner_changed = change.change_flags & (1 << 6) != 0;
            let lock_changed = change.type_index() == 0 && change.change_flags & (1 << 12) != 0;
            if !owner_changed
                && !lock_changed
                && !extra
                    .entries
                    .iter()
                    .any(|entry| matches!(entry.kind, 33..=35 | 42))
            {
                continue;
            }
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let state = self
                .refs
                .get(&id)
                .ok_or_else(|| bad(id, "restore reference placements first"))?;
            let mut owner = if owner_changed { None } else { state.owner };
            let mut rank = if owner_changed {
                None
            } else {
                state.owner_rank
            };
            let mut global = if owner_changed {
                None
            } else {
                state.owner_global
            };
            let mut lock = state.lock_extra;
            let mut level = state.lock_level;
            let mut locked = state.locked;
            if lock_changed {
                lock = Some(ReferenceLock::default());
                level = 0;
                locked = false;
            }
            let mut seen = BTreeSet::new();
            for entry in &extra.entries {
                if !matches!(entry.kind, 33..=35 | 42) {
                    continue;
                }
                if !seen.insert(entry.kind) {
                    return Err(bad(id, "duplicate ownership or lock extra"));
                }
                match (entry.kind, entry.fields.as_slice()) {
                    (33, [Field::Reference(value)]) => {
                        owner = mapping.reference(*value)?;
                        self.validate_extra_link(id, owner, &[T(b"NPC_"), T(b"FACT")])?;
                    }
                    (34, [Field::Reference(value)]) => {
                        global = mapping.reference(*value)?;
                        self.validate_extra_link(id, global, &[T(b"GLOB")])?;
                    }
                    (35, [Field::Word(value)]) => rank = Some(*value as i32),
                    (
                        42,
                        [Field::Byte(saved_level), Field::Byte(flags), Field::Reference(key), Field::Word(first), Field::Word(second)],
                    ) => {
                        let key = mapping.reference(*key)?;
                        self.validate_extra_link(id, key, &[T(b"KEYM")])?;
                        level = i32::from(*saved_level);
                        locked = flags & 1 != 0;
                        lock = Some(ReferenceLock {
                            key,
                            flags: flags & !1,
                            unknown: [*first, *second],
                        });
                        if flags & 4 != 0 {
                            report.leveled_locks.insert(id);
                        }
                    }
                    _ => return Err(bad(id, "invalid ownership or lock fields")),
                }
            }
            report.owners += usize::from(owner_changed || seen.contains(&33));
            report.locks += usize::from(lock_changed || seen.contains(&42));
            if prepared
                .insert(id, (owner, rank, global, lock, level, locked))
                .is_some()
            {
                return Err(bad(id, "duplicate reference extras target"));
            }
        }
        report.references = prepared.len();
        for (id, (owner, rank, global, lock, level, locked)) in prepared {
            if let Some(state) = self.refs.get_mut(&id) {
                state.owner = owner;
                state.owner_rank = rank;
                state.owner_global = global;
                state.lock_extra = lock;
                state.lock_level = level;
                state.locked = locked;
            }
        }
        Ok(report)
    }

    fn validate_extra_link(
        &self,
        target: GlobalFormId,
        link: Option<GlobalFormId>,
        tags: &[soe_esm::Tag],
    ) -> Result<(), RestoreError> {
        if let Some(link) = link {
            if !self
                .record(link)
                .is_some_and(|record| !record.is_deleted() && tags.contains(&record.tag))
            {
                return Err(bad(
                    target,
                    format!("invalid ownership or lock link {link}"),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_reference_extras(
        &self,
        refs: &HashMap<GlobalFormId, RefState>,
    ) -> Result<(), VmError> {
        for state in refs.values() {
            for (link, tag) in [
                (state.owner_global, T(b"GLOB")),
                (state.lock_extra.and_then(|lock| lock.key), T(b"KEYM")),
            ] {
                if let Some(link) = link {
                    if !self
                        .order
                        .winner(link)
                        .is_some_and(|record| !record.is_deleted() && record.tag == tag)
                    {
                        return Err(VmError::State("invalid reference extra link".into()));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn write_reference_extras(&self, writer: &mut Writer) {
        let states: BTreeMap<_, _> = self
            .refs
            .iter()
            .filter(|(_, state)| {
                state.lock_extra.is_some()
                    || state.owner_rank.is_some()
                    || state.owner_global.is_some()
            })
            .collect();
        writer.len(states.len());
        for (id, state) in states {
            write_form_id(writer, *id);
            write_extra_state(writer, state);
        }
    }

    pub(super) fn write_reference_flags(&self, writer: &mut Writer) {
        let states: BTreeMap<_, _> = self
            .refs
            .iter()
            .filter(|(id, state)| {
                state.record_flags != self.order.winner(**id).map_or(0, |record| record.flags)
            })
            .collect();
        writer.len(states.len());
        for (id, state) in states {
            write_form_id(writer, *id);
            writer.u32(state.record_flags);
        }
    }
}

pub(super) fn write_extra_state(w: &mut Writer, state: &RefState) {
    w.bool(state.owner_rank.is_some());
    if let Some(rank) = state.owner_rank {
        w.i32(rank);
    }
    write_opt_form_id(w, state.owner_global);
    w.bool(state.lock_extra.is_some());
    if let Some(lock) = state.lock_extra {
        write_opt_form_id(w, lock.key);
        w.u8(lock.flags);
        for word in lock.unknown {
            w.u32(word);
        }
    }
}

pub(super) fn read_reference_extras(
    r: &mut FormReader<'_, '_>,
    refs: &mut HashMap<GlobalFormId, RefState>,
) -> Result<(), VmError> {
    let count = r.len()?;
    if count > refs.len() {
        return Err(VmError::State("too many reference extras".into()));
    }
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(VmError::State("duplicate reference extras".into()));
        }
        let state = refs
            .get_mut(&id)
            .ok_or_else(|| VmError::State("reference extras without reference".into()))?;
        state.owner_rank = if r.bool()? { Some(r.i32()?) } else { None };
        state.owner_global = read_opt_form_id(r)?;
        state.lock_extra = if r.bool()? {
            let key = read_opt_form_id(r)?;
            let flags = r.u8()?;
            if flags & 1 != 0 || !(0..=255).contains(&state.lock_level) {
                return Err(VmError::State("invalid saved lock metadata".into()));
            }
            Some(ReferenceLock {
                key,
                flags,
                unknown: [r.u32()?, r.u32()?],
            })
        } else {
            None
        };
    }
    Ok(())
}

pub(super) fn read_reference_flags(
    r: &mut FormReader<'_, '_>,
    refs: &mut HashMap<GlobalFormId, RefState>,
) -> Result<(), VmError> {
    let count = r.len()?;
    if count > refs.len() {
        return Err(VmError::State("too many reference flag entries".into()));
    }
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(VmError::State("duplicate reference flag entry".into()));
        }
        let state = refs
            .get_mut(&id)
            .ok_or_else(|| VmError::State("reference flags without reference".into()))?;
        state.record_flags = r.u32()?;
    }
    Ok(())
}

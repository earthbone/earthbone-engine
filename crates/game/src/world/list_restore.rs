// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved FLST and leveled-list additions follow xEdit change types 43 to 46
//! and FallrimTools ChangeFormFLST, ChangeFormLeveled and LeveledEntry.

use std::collections::BTreeMap;

use soe_esm::{plugin::Subrecord, GlobalFormId};
use soe_save::Save;

use super::record_overrides::{raw_words, ListRecordExtra, RecordOverride};
use super::{forms::add_saved_form_list_entries, LeveledEntry, World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::List {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply all decoded FLST, LVLN, LVLI and LVSP changes to live records.
    /// A failed change leaves records and warm list caches unchanged.
    ///
    /// # Errors
    /// Rejects malformed payloads, unresolved identities, repeated or missing
    /// list records, colliding overrides and leveled entry-count overflow.
    pub fn restore_vanilla_lists(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        let mut form_additions = BTreeMap::new();
        let mut leveled_additions = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "FLST" => T(b"FLST"),
                "LVLN" => T(b"LVLN"),
                "LVLI" => T(b"LVLI"),
                "LVSP" => T(b"LVSP"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong list record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden list"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            let mut used = raw_words(record);
            let (flags, section_present, chances) = if tag == T(b"FLST") {
                let decoded = change
                    .form_list_change()?
                    .ok_or_else(|| invalid(id, "missing FLST change"))?;
                let section_present = decoded.forms.is_some();
                let mut additions = Vec::new();
                if let Some(forms) = decoded.forms {
                    for saved in forms {
                        let target = mapping.reference(saved)?;
                        let raw = value
                            .link(&mut used, target)
                            .map_err(|reason| invalid(id, reason))?;
                        value.record.subrecords.push(Subrecord {
                            tag: T(b"LNAM"),
                            data: raw.to_le_bytes().to_vec(),
                        });
                        additions.push(target.unwrap_or(GlobalFormId { file: 0, object: 0 }));
                    }
                }
                form_additions.insert(id, additions);
                (
                    decoded.form_flags.map(|f| (f.flags, f.unknown)),
                    section_present,
                    Vec::new(),
                )
            } else {
                let decoded = change
                    .leveled_list_change()?
                    .ok_or_else(|| invalid(id, "missing leveled change"))?;
                let section_present = decoded.entries.is_some();
                let mut additions = Vec::new();
                let mut chances = Vec::new();
                if let Some(entries) = decoded.entries {
                    let old_count = record.all(T(b"LVLO")).count();
                    let total = old_count
                        .checked_add(entries.len())
                        .ok_or_else(|| invalid(id, "entry count overflow"))?;
                    if total > 255 {
                        return Err(invalid(id, "more than 255 leveled entries"));
                    }
                    if record.find(T(b"LLCT")).is_some() {
                        value.set(T(b"LLCT"), [total as u8]);
                    }
                    for entry in entries {
                        let target = mapping.reference(entry.form)?;
                        let raw = value
                            .link(&mut used, target)
                            .map_err(|reason| invalid(id, reason))?;
                        let mut data = vec![0; 12];
                        data[..2].copy_from_slice(&u16::from(entry.level).to_le_bytes());
                        data[4..8].copy_from_slice(&raw.to_le_bytes());
                        data[8..10].copy_from_slice(&entry.count.to_le_bytes());
                        value.record.subrecords.push(Subrecord {
                            tag: T(b"LVLO"),
                            data,
                        });
                        additions.push(LeveledEntry {
                            level: u16::from(entry.level),
                            form: target.unwrap_or(GlobalFormId { file: 0, object: 0 }),
                            count: entry.count,
                        });
                        chances.push(entry.chance);
                    }
                }
                leveled_additions.insert(id, additions);
                (
                    decoded.form_flags.map(|f| (f.flags, f.unknown)),
                    section_present,
                    chances,
                )
            };
            value.list = Some(ListRecordExtra {
                form_flags: flags,
                section_present,
                entry_chances: chances,
            });
            let present = raw_words(&value.record);
            value.links.retain(|raw, _| present.contains(raw));
            prepared.insert(id, value);
        }
        for (&id, additions) in &leveled_additions {
            if let Some(cached) = self.leveled_lists.get(&id) {
                if cached.base_entries > cached.entries.len()
                    || cached.base_entries.checked_add(additions.len()).is_none()
                {
                    return Err(invalid(id, "invalid warm leveled list state"));
                }
            }
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        for (id, additions) in form_additions {
            if let Some(cached) = self.form_lists.get_mut(&id) {
                add_saved_form_list_entries(cached, additions);
            }
        }
        for (id, additions) in leveled_additions {
            if let Some(cached) = self.leveled_lists.get_mut(&id) {
                let at = cached.base_entries;
                cached.base_entries += additions.len();
                cached.entries.splice(at..at, additions);
            }
        }
        Ok(count)
    }
}

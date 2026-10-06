// SPDX-License-Identifier: GPL-3.0-or-later

//! BOOK, NOTE and KEYM save changes applied to shared records. Field meaning
//! follows xEdit `Core/wbDefinitionsTES5Saves.pas`.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{BookNoteKeyRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::BookNoteKey {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply decoded BOOK, NOTE and KEYM changes to the shared winning records.
    /// A failed change leaves all record overrides unchanged.
    ///
    /// # Errors
    /// Rejects invalid mappings, duplicate changes, wrong or missing records,
    /// and records already changed by another restoration path.
    pub fn restore_vanilla_book_note_keys(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "BOOK" => T(b"BOOK"),
                "NOTE" => T(b"NOTE"),
                "KEYM" => T(b"KEYM"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong base record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden record"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            let extra = match change.record_type() {
                "BOOK" => {
                    let decoded = change
                        .book_change()?
                        .ok_or_else(|| invalid(id, "missing BOOK data"))?;
                    BookNoteKeyRecordExtra {
                        form_flags: decoded.form_flags.map(|f| (f.flags, f.unknown)),
                        value_changed: decoded.value_changed,
                        full_name_changed: decoded.full_name_changed,
                        teaches_skill: decoded.teaches_skill,
                        read: decoded.read,
                        opaque_suffix: decoded.opaque_suffix,
                    }
                }
                "NOTE" => {
                    let decoded = change
                        .note_change()?
                        .ok_or_else(|| invalid(id, "missing NOTE data"))?;
                    BookNoteKeyRecordExtra {
                        form_flags: decoded.form_flags.map(|f| (f.flags, f.unknown)),
                        read: decoded.read,
                        opaque_suffix: decoded.opaque_suffix,
                        ..BookNoteKeyRecordExtra::default()
                    }
                }
                _ => {
                    let decoded = change
                        .key_change()?
                        .ok_or_else(|| invalid(id, "missing KEYM data"))?;
                    BookNoteKeyRecordExtra {
                        form_flags: decoded.form_flags.map(|f| (f.flags, f.unknown)),
                        value_changed: decoded.value_changed,
                        full_name_changed: decoded.full_name_changed,
                        opaque_suffix: decoded.opaque_suffix,
                        ..BookNoteKeyRecordExtra::default()
                    }
                }
            };
            if let Some((flags, _)) = extra.form_flags {
                value.record.flags ^= flags;
            }
            value.book_note_key = Some(extra);
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

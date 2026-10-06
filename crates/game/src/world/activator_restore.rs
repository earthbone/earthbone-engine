// SPDX-License-Identifier: GPL-3.0-or-later

//! ACTI, TACT and DOOR changes follow xEdit's zero-sized marker fields.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{ActivatorRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Activator {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply saved ACTI, TACT and DOOR flags to their shared live records.
    /// Marker-only fields remain available through `restored_activator_extra`.
    /// Changes are validated before any override becomes visible.
    ///
    /// # Errors
    /// Invalid mappings, missing or wrong records, duplicate changes, or
    /// malformed payloads leave all records unchanged.
    pub fn restore_vanilla_activators(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            if !matches!(change.record_type(), "ACTI" | "TACT" | "DOOR") {
                continue;
            }
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let tag = match change.record_type() {
                "ACTI" => T(b"ACTI"),
                "TACT" => T(b"TACT"),
                _ => T(b"DOOR"),
            };
            let original = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong activator record"))?;
            if self.record_overrides.contains_key(&id) || prepared.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden activator"));
            }
            let decoded = change
                .activator_change()?
                .ok_or_else(|| invalid(id, "missing activator change"))?;
            let mut value = RecordOverride::new(id, original.clone());
            if let Some(flags) = decoded.form_flags {
                value.record.flags ^= flags.flags;
            }
            value.activator = Some(ActivatorRecordExtra {
                form_flags: decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                value_changed: decoded.value_changed,
                full_name_changed: decoded.full_name_changed,
                speaker_changed: decoded.speaker_changed,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }

    /// Return the retained marker fields for a restored base activator.
    pub fn restored_activator_extra(&self, id: GlobalFormId) -> Option<&ActivatorRecordExtra> {
        self.record_overrides.get(&id)?.activator.as_ref()
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply the LIGH, STAT and MSTT fields xEdit defines in saved change forms.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{BaseObjectRecordExtra, RecordOverride};
use super::World;
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::BaseObject {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply saved LIGH, STAT and MSTT fields to the shared record view.
    ///
    /// xEdit defines value and full-name markers without payload bytes. Their
    /// markers and optional form flags remain available as record metadata.
    /// The pass validates every matching change before replacing any record.
    ///
    /// # Errors
    /// Rejects malformed changes, unresolved identities, wrong record types,
    /// duplicate changes and records already overridden by another restore.
    pub fn restore_vanilla_base_objects(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let expected = match change.record_type() {
                "LIGH" => super::T(b"LIGH"),
                "STAT" => super::T(b"STAT"),
                "MSTT" => super::T(b"MSTT"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == expected && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong base-object record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden base object"));
            }
            let decoded = change
                .light_static_change()?
                .ok_or_else(|| invalid(id, "missing base-object change"))?;
            let mut value = RecordOverride::new(id, record.clone());
            value.base_object = Some(BaseObjectRecordExtra {
                form_flags: decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                value_changed: decoded.value_changed,
                full_name_changed: decoded.full_name_changed,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

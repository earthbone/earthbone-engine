// SPDX-License-Identifier: GPL-3.0-or-later

//! APPA and IDLM change forms follow xEdit `wbDefinitionsTES5Saves.pas`.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{BaseObjectRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::BaseObject {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply saved APPA and IDLM form flags to their live records.
    ///
    /// The saved value and full-name fields are zero-byte unions in xEdit's
    /// layout. Their change bits are retained with the override metadata.
    ///
    /// # Errors
    /// Rejects malformed payloads, unresolved records, duplicate changes and
    /// records that already have an override. No changes are applied on error.
    ///
    /// # Panics
    /// Does not panic for malformed save data.
    pub fn restore_vanilla_base_objects(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "APPA" => T(b"APPA"),
                "IDLM" => T(b"IDLM"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let original = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong base object"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden base object"));
            }
            let decoded = if tag == T(b"APPA") {
                change.apparatus_change()?
            } else {
                change.idle_marker_change()?
            }
            .ok_or_else(|| invalid(id, "missing decoded change"))?;
            let mut value = RecordOverride::new(id, original.clone());
            if let Some((mask_invert, _)) = decoded.form_flags {
                value.record.flags ^= mask_invert;
            }
            value.base_object = Some(BaseObjectRecordExtra {
                form_flags: decoded.form_flags,
                change_flags: decoded.change_flags,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

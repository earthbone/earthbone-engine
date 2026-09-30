// SPDX-License-Identifier: GPL-3.0-or-later

//! CONT, MISC and FURN save changes follow xEdit's save definitions.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{RecordOverride, StaticObjectRecordExtra};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::StaticObject {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply decoded CONT, MISC and FURN changes to winning records.
    ///
    /// The xEdit form-flags word is an inversion mask. The zero-byte value
    /// and full-name markers and any suffix bytes remain attached to the
    /// override for save/load preservation.
    ///
    /// # Errors
    /// Returns an error for invalid saved identities, malformed changes,
    /// missing or mismatched records, and duplicate or already overridden ids.
    pub fn restore_vanilla_static_objects(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "CONT" => T(b"CONT"),
                "MISC" => T(b"MISC"),
                "FURN" => T(b"FURN"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let original = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong static-object record"))?;
            if self.record_overrides.contains_key(&id) || prepared.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden static object"));
            }
            let decoded = change
                .static_object_change()?
                .ok_or_else(|| invalid(id, "missing static-object change"))?;
            let mut value = RecordOverride::new(id, original.clone());
            if let Some((mask, _)) = decoded.form_flags {
                value.record.flags ^= mask;
            }
            value.static_object = Some(StaticObjectRecordExtra {
                change_flags: decoded.change_flags,
                form_flags: decoded.form_flags,
                value_changed: decoded.value_changed,
                full_name_changed: decoded.full_name_changed,
                opaque_suffix: decoded.opaque_suffix,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

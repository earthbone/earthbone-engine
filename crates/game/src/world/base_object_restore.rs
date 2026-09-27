// SPDX-License-Identifier: GPL-3.0-or-later

//! WOOP and MGEF changes retain xEdit's base-object markers in shared records.

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
    /// Apply decoded WOOP and MGEF change markers to shared record state.
    /// The markers' value and full-name sections carry no payload bytes.
    /// Every change is decoded and checked before any override is installed.
    ///
    /// # Errors
    /// Rejects invalid identities, malformed changes, duplicate forms and
    /// records that are absent, deleted, mistyped or already overridden.
    pub fn restore_vanilla_base_objects(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let (tag, decoded) = match change.record_type() {
                "WOOP" => (T(b"WOOP"), change.word_of_power_change()?),
                "MGEF" => (T(b"MGEF"), change.magic_effect_change()?),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let decoded = decoded.ok_or_else(|| invalid(id, "missing decoded change"))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong base-object record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(
                    id,
                    "duplicate or already overridden base-object record",
                ));
            }
            if tag == T(b"MGEF") {
                soe_esm::records::gameplay::MagicEffect::from_record(
                    record,
                    self.order
                        .winner_ref(id)
                        .is_some_and(|r| self.order.file(r.file).plugin.header.is_localized()),
                )
                .map_err(|error| invalid(id, error.to_string()))?;
            }
            let mut value = RecordOverride::new(id, record.clone());
            value.base_object = Some(BaseObjectRecordExtra {
                form_flags: decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                value_changed: decoded.base_object_value_changed,
                full_name_changed: decoded.base_object_fullname_changed,
                unnamed_flags: decoded.unnamed_flags,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

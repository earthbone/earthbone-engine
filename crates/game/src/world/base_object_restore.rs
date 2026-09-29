// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply saved fields for Skyrim base-object change forms.

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
    /// Apply saved base-object and WOOP/MGEF change markers to winning records.
    /// Every matching change is validated before any override is installed.
    ///
    /// # Errors
    /// Rejects malformed changes, unresolved identities, wrong record types,
    /// duplicate changes and records already overridden by another restore.
    pub fn restore_vanilla_base_objects(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "LIGH" => T(b"LIGH"),
                "STAT" => T(b"STAT"),
                "MSTT" => T(b"MSTT"),
                "APPA" => T(b"APPA"),
                "IDLM" => T(b"IDLM"),
                "WOOP" => T(b"WOOP"),
                "MGEF" => T(b"MGEF"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
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
            let (form_flags, change_flags, value_changed, full_name_changed, unnamed_flags) =
                match &tag.0 {
                    b"LIGH" | b"STAT" | b"MSTT" => {
                        let decoded = change
                            .light_static_change()?
                            .ok_or_else(|| invalid(id, "missing base-object change"))?;
                        (
                            decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                            decoded.change_flags(),
                            decoded.value_changed,
                            decoded.full_name_changed,
                            0,
                        )
                    }
                    b"APPA" => {
                        let decoded = change
                            .apparatus_change()?
                            .ok_or_else(|| invalid(id, "missing base-object change"))?;
                        (
                            decoded.form_flags,
                            decoded.change_flags,
                            decoded.value_changed,
                            decoded.full_name_changed,
                            0,
                        )
                    }
                    b"IDLM" => {
                        let decoded = change
                            .idle_marker_change()?
                            .ok_or_else(|| invalid(id, "missing base-object change"))?;
                        (
                            decoded.form_flags,
                            decoded.change_flags,
                            decoded.value_changed,
                            decoded.full_name_changed,
                            0,
                        )
                    }
                    _ => {
                        let decoded = if tag == T(b"WOOP") {
                            change.word_of_power_change()?
                        } else {
                            change.magic_effect_change()?
                        }
                        .ok_or_else(|| invalid(id, "missing decoded change"))?;
                        if tag == T(b"MGEF") {
                            soe_esm::records::gameplay::MagicEffect::from_record(
                                record,
                                self.order.winner_ref(id).is_some_and(|r| {
                                    self.order.file(r.file).plugin.header.is_localized()
                                }),
                            )
                            .map_err(|error| invalid(id, error.to_string()))?;
                        }
                        (
                            decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                            decoded.change_flags(),
                            decoded.base_object_value_changed,
                            decoded.base_object_fullname_changed,
                            decoded.unnamed_flags,
                        )
                    }
                };
            let mut value = RecordOverride::new(id, record.clone());
            if matches!(&tag.0, b"APPA" | b"IDLM") {
                if let Some((mask, _)) = form_flags {
                    value.record.flags ^= mask;
                }
            }
            value.base_object = Some(BaseObjectRecordExtra {
                form_flags,
                change_flags,
                value_changed,
                full_name_changed,
                unnamed_flags,
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved ARMO, WEAP and AMMO base-object fields follow xEdit's saved-form schema.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::{ItemChange, Save};

use super::record_overrides::{ItemRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Item {
        id,
        reason: reason.into(),
    }
}

fn extra(change: ItemChange) -> ItemRecordExtra {
    ItemRecordExtra {
        form_flags: change.form_flags,
        value_changed: change.value_changed,
        full_name_changed: change.full_name_changed,
        unnamed_flags: change.unnamed_flags,
        trailing_data: change.trailing_data,
    }
}

impl World {
    /// Apply decoded item changes to the shared ARMO, WEAP and AMMO record table.
    ///
    /// xEdit defines value and full-name markers as zero-byte fields. The
    /// record remains the winning plugin record while every decoded field is
    /// retained in its override metadata for independent queries and saves.
    ///
    /// # Errors
    /// Rejects malformed payloads, unresolved identities, duplicate changes,
    /// existing overrides, and missing, deleted or mistyped records.
    pub fn restore_vanilla_items(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let Some(decoded) = change.item_change()? else {
                continue;
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let tag = match decoded.record_type {
                "ARMO" => T(b"ARMO"),
                "WEAP" => T(b"WEAP"),
                "AMMO" => T(b"AMMO"),
                _ => return Err(invalid(id, "unsupported item record type")),
            };
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong item record"))?;
            if self.record_overrides.contains_key(&id) || prepared.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden item record"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            value.item = Some(extra(decoded));
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

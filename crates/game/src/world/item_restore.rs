// SPDX-License-Identifier: GPL-3.0-or-later

//! INGR, ALCH and ENCH save markers follow xEdit's
//! `wbDefinitionsTES5Saves.pas` change-form definitions.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{ItemRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Item {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply decoded INGR, ALCH and ENCH changes to the shared record table.
    /// Every target is checked before any record becomes visible.
    ///
    /// # Errors
    /// Invalid saved identities, duplicate changes, missing or mismatched
    /// records, existing overrides or malformed typed change data.
    pub fn restore_vanilla_items(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "INGR" => T(b"INGR"),
                "ALCH" => T(b"ALCH"),
                "ENCH" => T(b"ENCH"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong item record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden item"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            value.item = Some(match tag.as_bytes() {
                b"INGR" => {
                    let change = change
                        .ingredient_change()?
                        .ok_or_else(|| invalid(id, "missing INGR change"))?;
                    if let Some(flags) = change.form_flags {
                        value.record.flags = flags.flags;
                    }
                    ItemRecordExtra {
                        form_flags: change.form_flags.map(|flags| (flags.flags, flags.unknown)),
                        ingredient_use_changed: change.ingredient_use_changed,
                        value_changed: false,
                        fullname_changed: false,
                        other_flags: change.other_flags,
                    }
                }
                b"ALCH" => {
                    let change = change
                        .alchemy_change()?
                        .ok_or_else(|| invalid(id, "missing ALCH change"))?;
                    if let Some(flags) = change.form_flags {
                        value.record.flags = flags.flags;
                    }
                    ItemRecordExtra {
                        form_flags: change.form_flags.map(|flags| (flags.flags, flags.unknown)),
                        ingredient_use_changed: false,
                        value_changed: change.value_changed,
                        fullname_changed: change.fullname_changed,
                        other_flags: change.other_flags,
                    }
                }
                b"ENCH" => {
                    let change = change
                        .enchantment_change()?
                        .ok_or_else(|| invalid(id, "missing ENCH change"))?;
                    if let Some(flags) = change.form_flags {
                        value.record.flags = flags.flags;
                    }
                    ItemRecordExtra {
                        form_flags: change.form_flags.map(|flags| (flags.flags, flags.unknown)),
                        ingredient_use_changed: false,
                        value_changed: change.value_changed,
                        fullname_changed: change.fullname_changed,
                        other_flags: change.other_flags,
                    }
                }
                _ => return Err(invalid(id, "unsupported item record type")),
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

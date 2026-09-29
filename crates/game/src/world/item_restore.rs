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
        ingredient_use_changed: false,
        fullname_changed: false,
        unnamed_flags: change.unnamed_flags,
        other_flags: 0,
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
            let kind = change.record_type();
            let tag = match kind {
                "ARMO" => T(b"ARMO"),
                "WEAP" => T(b"WEAP"),
                "AMMO" => T(b"AMMO"),
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
                .filter(|r| r.tag == tag && !r.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong item record"))?;
            if self.record_overrides.contains_key(&id) || prepared.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden item record"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            value.item = Some(match kind {
                "ARMO" | "WEAP" | "AMMO" => extra(
                    change
                        .item_change()?
                        .ok_or_else(|| invalid(id, "missing item change"))?,
                ),
                "INGR" => {
                    let decoded = change
                        .ingredient_change()?
                        .ok_or_else(|| invalid(id, "missing INGR change"))?;
                    if let Some(flags) = decoded.form_flags {
                        value.record.flags = flags.flags;
                    }
                    ItemRecordExtra {
                        form_flags: decoded.form_flags.map(|f| (f.flags, f.unknown)),
                        ingredient_use_changed: decoded.ingredient_use_changed,
                        other_flags: decoded.other_flags,
                        ..ItemRecordExtra::default()
                    }
                }
                "ALCH" | "ENCH" => {
                    let decoded = if kind == "ALCH" {
                        change.alchemy_change()?
                    } else {
                        change.enchantment_change()?
                    }
                    .ok_or_else(|| invalid(id, "missing base object change"))?;
                    if let Some(flags) = decoded.form_flags {
                        value.record.flags = flags.flags;
                    }
                    ItemRecordExtra {
                        form_flags: decoded.form_flags.map(|f| (f.flags, f.unknown)),
                        value_changed: decoded.value_changed,
                        fullname_changed: decoded.fullname_changed,
                        other_flags: decoded.other_flags,
                        ..ItemRecordExtra::default()
                    }
                }
                _ => unreachable!(),
            });
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

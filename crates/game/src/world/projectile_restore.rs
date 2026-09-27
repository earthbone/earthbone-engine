// SPDX-License-Identifier: GPL-3.0-or-later

//! PFLA, PHZD and PBAR saved change envelopes follow local TES5Edit
//! `wbDefinitionsTES5Saves.pas`. That source names shared flags but leaves
//! these subtype payload structures empty.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{ProjectileRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Projectile {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply PFLA, PHZD and PBAR saved change envelopes to the live form table.
    /// The shared flags and opaque subtype bytes remain available to runtime
    /// consumers through [`World::restored_projectile_extra`].
    ///
    /// # Errors
    /// Rejects malformed payloads, unresolved targets, wrong record types,
    /// duplicate changes and records already carrying a runtime override.
    pub fn restore_vanilla_projectiles(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "PFLA" => T(b"PFLA"),
                "PHZD" => T(b"PHZD"),
                "PBAR" => T(b"PBAR"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong projectile record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden projectile"));
            }
            let decoded = change
                .projectile_change()?
                .ok_or_else(|| invalid(id, "missing projectile change"))?;
            let mut override_record = RecordOverride::new(id, record.clone());
            override_record.projectile = Some(ProjectileRecordExtra {
                flags: decoded.flags.bits(),
                payload: decoded.payload,
            });
            prepared.insert(id, override_record);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

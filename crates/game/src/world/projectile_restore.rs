// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved projectile envelopes follow local TES5Edit
//! `wbDefinitionsTES5Saves.pas`. That source leaves projectile subtype
//! payload structures empty, so runtime consumers receive the exact bytes.

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
    /// Apply projectile saved change envelopes to the live form table.
    /// PFLA, PHZD and PBAR use their shared flags, while PCON and PARW retain
    /// their envelope flags and exact opaque payload bytes. Runtime consumers
    /// read all five through [`World::restored_projectile_extra`].
    ///
    /// # Errors
    /// Rejects malformed payloads, unresolved targets, wrong record types,
    /// duplicate changes and records already carrying a runtime override.
    pub fn restore_vanilla_projectiles(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let (tag, decoded) = match change.record_type() {
                "PFLA" => (
                    T(b"PFLA"),
                    change
                        .projectile_change()?
                        .map(|v| (v.flags.bits(), v.payload)),
                ),
                "PHZD" => (
                    T(b"PHZD"),
                    change
                        .projectile_change()?
                        .map(|v| (v.flags.bits(), v.payload)),
                ),
                "PBAR" => (
                    T(b"PBAR"),
                    change
                        .projectile_change()?
                        .map(|v| (v.flags.bits(), v.payload)),
                ),
                "PCON" => (
                    T(b"PCON"),
                    change.pcon_change().map(|v| (v.change_flags, v.data)),
                ),
                "PARW" => (
                    T(b"PARW"),
                    change.parw_change().map(|v| (v.change_flags, v.data)),
                ),
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
            let (flags, payload) =
                decoded.ok_or_else(|| invalid(id, "missing projectile change"))?;
            let mut override_record = RecordOverride::new(id, record.clone());
            override_record.projectile = Some(ProjectileRecordExtra { flags, payload });
            prepared.insert(id, override_record);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

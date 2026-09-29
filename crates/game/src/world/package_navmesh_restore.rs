// SPDX-License-Identifier: GPL-3.0-or-later

//! Restore saved PACK and NAVM change markers onto their shared records.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::record_overrides::{NavmeshRecordExtra, PackRecordExtra, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::WorldReference {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply decoded PACK and NAVM save markers to the live record table.
    /// Validate the complete batch before adding record overrides.
    ///
    /// # Errors
    /// Invalid plugin mapping, duplicate or existing overrides, missing or
    /// mistyped forms, malformed payloads, or invalid record-backed consumers.
    pub fn restore_vanilla_pack_navmesh(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        for change in &save.change_forms {
            let tag = match change.record_type() {
                "PACK" => T(b"PACK"),
                "NAVM" => T(b"NAVM"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|record| record.tag == tag && !record.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or wrong record"))?;
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden record"));
            }
            let mut value = RecordOverride::new(id, record.clone());
            if tag == T(b"PACK") {
                let decoded = change
                    .pack_change()?
                    .ok_or_else(|| invalid(id, "missing PACK change"))?;
                crate::package::Package::from_record(&value.record)
                    .map_err(|error| invalid(id, error.to_string()))?;
                value.pack = Some(PackRecordExtra {
                    form_flags: decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                    waiting: decoded.waiting,
                    never_run: decoded.never_run,
                });
            } else {
                let decoded = change
                    .navmesh_change()?
                    .ok_or_else(|| invalid(id, "missing NAVM change"))?;
                soe_esm::records::navm::NavMesh::from_record(&value.record)
                    .map_err(|error| invalid(id, error.to_string()))?;
                value.navmesh = Some(NavmeshRecordExtra {
                    form_flags: decoded.form_flags.map(|flags| (flags.flags, flags.unknown)),
                    value_changed: decoded.value_changed,
                    full_name_changed: decoded.full_name_changed,
                });
            }
            if prepared.insert(id, value).is_some() {
                return Err(invalid(id, "duplicate saved change"));
            }
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        Ok(count)
    }
}

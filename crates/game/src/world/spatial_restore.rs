// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply the spatial change-form fields whose save layouts are defined.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;
use soe_save::Save;

use super::record_overrides::{raw_words, RecordOverride};
use super::save::{read_form_id, write_form_id, FormReader};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl std::fmt::Display) -> RestoreError {
    RestoreError::SpatialRecord {
        id,
        reason: reason.to_string(),
    }
}

impl World {
    /// Apply saved CELL, LCTN and ECZN fields to the shared winning records.
    /// The entire batch is decoded and validated before any record is replaced.
    /// CELL fields use xEdit's defined layout. LCTN and ECZN placeholder tails
    /// remain opaque, while their common form flags update the live record.
    /// No events are dispatched.
    ///
    /// # Errors
    /// Invalid saved identities, unsupported change flags, malformed payloads,
    /// duplicate targets or targets that do not name live records of the type.
    pub fn restore_vanilla_spatial_records(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        let mut cells = BTreeSet::new();
        let mut spatial_payloads = BTreeMap::new();
        for change in &save.change_forms {
            let kind = change.record_type();
            if !matches!(kind, "CELL" | "LCTN" | "ECZN") {
                continue;
            }
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            if prepared.contains_key(&id) {
                return Err(invalid(id, "duplicate spatial change form"));
            }
            let original = self
                .order
                .winner(id)
                .filter(|record| {
                    !record.is_deleted()
                        && record.tag
                            == Tag(match kind {
                                "CELL" => *b"CELL",
                                "LCTN" => *b"LCTN",
                                "ECZN" => *b"ECZN",
                                _ => unreachable!(),
                            })
                })
                .ok_or_else(|| invalid(id, "target is missing, deleted or has another type"))?;
            let mut value = self
                .record_overrides
                .get(&id)
                .cloned()
                .unwrap_or_else(|| RecordOverride::new(id, original.clone()));
            match kind {
                "CELL" => {
                    let decoded = change
                        .cell_change()?
                        .ok_or_else(|| invalid(id, "CELL decoder returned no change"))?;
                    if let Some(flags) = decoded.form_flags {
                        value.record.flags = flags.flags;
                        value.npc.pending_form_flags = Some((flags.flags, flags.unknown));
                    }
                    if let Some(flags) = decoded.cell_flags {
                        let mut data = value.record.find(T(b"DATA")).unwrap_or_default().to_vec();
                        if data.len() < 2 {
                            data.resize(2, 0);
                        }
                        data[..2].copy_from_slice(&flags.to_le_bytes());
                        value.set(T(b"DATA"), data);
                    }
                    if let Some(name) = decoded.full_name {
                        let mut bytes = name.to_text().into_bytes();
                        bytes.push(0);
                        value.set(T(b"FULL"), bytes);
                        value.plain_full = true;
                    }
                    if let Some(owner) = decoded.owner {
                        let owner = mapping.reference(owner)?;
                        if let Some(raw) = value
                            .record
                            .find(T(b"XOWN"))
                            .and_then(|bytes| bytes.try_into().ok())
                            .map(u32::from_le_bytes)
                        {
                            value.links.remove(&raw);
                        }
                        let mut used = raw_words(&value.record);
                        let raw = value.link(&mut used, owner).map_err(|e| invalid(id, e))?;
                        value.set(T(b"XOWN"), raw.to_le_bytes());
                    }
                    cells.insert(id);
                }
                "LCTN" => {
                    let decoded = change
                        .location_change()?
                        .ok_or_else(|| invalid(id, "LCTN decoder returned no change"))?;
                    if let Some(flags) = decoded.form_flags {
                        value.record.flags = flags.flags;
                        value.npc.pending_form_flags = Some((flags.flags, flags.unknown));
                    }
                    spatial_payloads.insert(id, decoded.opaque_payload);
                }
                "ECZN" => {
                    let decoded = change
                        .encounter_zone_change()?
                        .ok_or_else(|| invalid(id, "ECZN decoder returned no change"))?;
                    if let Some(flags) = decoded.form_flags {
                        value.record.flags = flags.flags;
                        value.npc.pending_form_flags = Some((flags.flags, flags.unknown));
                    }
                    spatial_payloads.insert(id, decoded.opaque_payload);
                }
                _ => unreachable!(),
            }
            prepared.insert(id, value);
        }
        let count = prepared.len();
        self.record_overrides.extend(prepared);
        self.spatial_change_payloads.extend(spatial_payloads);
        for id in cells {
            self.cells.remove(&id);
        }
        Ok(count)
    }
}

impl World {
    /// Return the retained opaque LCTN or ECZN change payload.
    #[must_use]
    pub fn restored_spatial_payload(&self, id: GlobalFormId) -> Option<&[u8]> {
        self.spatial_change_payloads.get(&id).map(Vec::as_slice)
    }
}

pub(super) fn write_spatial_payloads(world: &World, w: &mut Writer) {
    w.len(world.spatial_change_payloads.len());
    for (id, payload) in &world.spatial_change_payloads {
        write_form_id(w, *id);
        w.bytes(payload);
    }
}

pub(super) fn read_spatial_payloads(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let count = r.len()?;
    if count > r.remaining() / 10 {
        return Err(VmError::State(
            "spatial payload count exceeds remaining bytes".into(),
        ));
    }
    let mut payloads = BTreeMap::new();
    for _ in 0..count {
        let id = read_form_id(r)?;
        let valid = world.order.winner(id).is_some_and(|record| {
            matches!(&record.tag.0, b"LCTN" | b"ECZN") && !record.is_deleted()
        });
        if !valid || payloads.insert(id, r.bytes()?.to_vec()).is_some() {
            return Err(VmError::State("invalid spatial payload identity".into()));
        }
    }
    world.spatial_change_payloads = payloads;
    Ok(())
}

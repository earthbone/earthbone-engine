// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW record overrides. Original plugins stay immutable and keep link provenance.

use std::collections::BTreeMap;

use soe_esm::{plugin::Subrecord, GlobalFormId, Slot, Tag};
use soe_papyrus::{vm::state::Writer, VmError};

use super::record_overrides::{raw_words, NpcRecordExtra, RecordOverride};
use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{World, T};

fn bad(reason: &str) -> VmError {
    VmError::State(format!("record override: {reason}"))
}

impl World {
    fn saved_record_identity(&self, id: GlobalFormId) -> Result<(), VmError> {
        let limit = if id.file == super::DYNAMIC_FILE {
            0xFFFFFF
        } else {
            match self
                .order
                .files()
                .get(usize::from(id.file))
                .map(|file| file.slot)
            {
                Some(Slot::Regular(_)) => 0xFFFFFF,
                Some(Slot::Light(_)) => 0xFFF,
                None => return Err(bad("link file is outside the load order")),
            }
        };
        if id.object > limit {
            return Err(bad("link object exceeds its identity space"));
        }
        Ok(())
    }

    pub(super) fn write_record_overrides(&self, w: &mut Writer) {
        w.len(self.record_overrides.len());
        for (id, value) in &self.record_overrides {
            write_form_id(w, *id);
            write_form_id(w, value.origin);
            write_form_id(w, value.text_origin);
            write_opt_form_id(w, value.temporary_for);
            w.bool(value.plain_full);
            write_opt_form_id(w, value.npc.original_race);
            w.bool(value.npc.body_tint.is_some());
            if let Some(tint) = value.npc.body_tint {
                w.out.extend(tint);
            }
            w.bool(value.npc.pending_form_flags.is_some());
            if let Some((flags, unknown)) = value.npc.pending_form_flags {
                w.u32(flags);
                w.u16(unknown);
            }
            w.len(value.links.len());
            for (raw, link) in &value.links {
                w.u32(*raw);
                write_form_id(w, *link);
            }
            w.len(value.record.subrecords.len());
            for sub in &value.record.subrecords {
                w.out.extend(sub.tag.0);
                w.bytes(&sub.data);
            }
        }
    }

    pub(super) fn read_record_overrides(
        &self,
        r: &mut FormReader<'_, '_>,
        version: u16,
    ) -> Result<BTreeMap<GlobalFormId, RecordOverride>, VmError> {
        let mut values = BTreeMap::new();
        let count = r.len()?;
        if count > r.remaining() / 18 {
            return Err(bad("record count exceeds remaining bytes"));
        }
        for _ in 0..count {
            let id = read_form_id(r)?;
            self.saved_record_identity(id)?;
            let origin = if version >= 14 { read_form_id(r)? } else { id };
            let text_origin = if version >= 14 { read_form_id(r)? } else { id };
            let temporary_for = if version >= 14 {
                read_opt_form_id(r)?
            } else {
                None
            };
            if (temporary_for.is_some() && id.file != super::DYNAMIC_FILE)
                || (temporary_for.is_none() && origin != id)
                || self
                    .order
                    .winner(text_origin)
                    .is_none_or(|r| r.tag != T(b"NPC_"))
            {
                return Err(bad("invalid NPC origin or temporary identity"));
            }
            let original = self
                .order
                .winner(origin)
                .filter(|rec| rec.tag == T(b"NPC_") && !rec.is_deleted())
                .ok_or_else(|| bad("missing NPC origin"))?;
            let mut record = original.clone();
            if temporary_for.is_some() {
                record.form_id = 0xFF00_0000 | id.object;
            }
            let plain_full = r.bool()?;
            let original_race = read_opt_form_id(r)?;
            if let Some(id) = original_race {
                self.saved_record_identity(id)?;
            }
            let body_tint = if r.bool()? {
                Some([r.u8()?, r.u8()?, r.u8()?, r.u8()?])
            } else {
                None
            };
            let pending_form_flags = if r.bool()? {
                Some((r.u32()?, r.u16()?))
            } else {
                None
            };
            let mut links = BTreeMap::new();
            let used = raw_words(original);
            let count = r.len()?;
            if count > r.remaining() / 10 {
                return Err(bad("link count exceeds remaining bytes"));
            }
            for _ in 0..count {
                let raw = r.u32()?;
                let link = read_form_id(r)?;
                self.saved_record_identity(link)?;
                if used.contains(&raw) || links.insert(raw, link).is_some() {
                    return Err(bad("duplicate or colliding link token"));
                }
            }
            let count = r.len()?;
            if count > r.remaining() / 8 {
                return Err(bad("subrecord count exceeds remaining bytes"));
            }
            let mut subrecords = vec![];
            for _ in 0..count {
                let tag = Tag([r.u8()?, r.u8()?, r.u8()?, r.u8()?]);
                let data = r.bytes()?.to_vec();
                subrecords.push(Subrecord { tag, data });
            }
            record.subrecords = subrecords;
            let present = raw_words(&record);
            if links.keys().any(|raw| !present.contains(raw)) {
                return Err(bad("unused link token"));
            }
            if record.editor_id() != original.editor_id() {
                return Err(bad("NPC override changed editor identity"));
            }
            let localized = !plain_full
                && self
                    .order
                    .winner_ref(text_origin)
                    .is_some_and(|owner| self.order.file(owner.file).plugin.header.is_localized());
            crate::npc::Npc::from_record(&record, localized)
                .map_err(|e| VmError::State(e.to_string()))?;
            let value = RecordOverride {
                origin,
                text_origin,
                temporary_for,
                record,
                links,
                plain_full,
                npc: NpcRecordExtra {
                    original_race,
                    body_tint,
                    pending_form_flags,
                },
            };
            if values.insert(id, value).is_some() {
                return Err(bad("duplicate NPC record"));
            }
        }
        Ok(values)
    }
}

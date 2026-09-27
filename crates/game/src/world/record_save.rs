// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW record overrides. Original plugins stay immutable and keep link provenance.

use std::collections::BTreeMap;

use soe_esm::{plugin::Subrecord, GlobalFormId, Slot, Tag};
use soe_papyrus::{vm::state::Writer, VmError};

use super::record_overrides::{
    raw_words, BaseObjectRecordExtra, ListRecordExtra, NpcRecordExtra, RecordOverride,
};
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
            w.bool(value.list.is_some());
            if let Some(list) = &value.list {
                w.bool(list.form_flags.is_some());
                if let Some((flags, unknown)) = list.form_flags {
                    w.u32(flags);
                    w.u16(unknown);
                }
                w.bool(list.section_present);
                w.bytes(&list.entry_chances);
            }
            w.bool(value.base_object.is_some());
            if let Some(extra) = value.base_object {
                w.bool(extra.form_flags.is_some());
                if let Some((flags, unknown)) = extra.form_flags {
                    w.u32(flags);
                    w.u16(unknown);
                }
                w.bool(extra.value_changed);
                w.bool(extra.full_name_changed);
                w.u32(extra.unnamed_flags);
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
                || self.order.winner(text_origin).is_none_or(|r| {
                    !matches!(
                        &r.tag.0,
                        b"NPC_"
                            | b"RACE"
                            | b"FLST"
                            | b"LVLN"
                            | b"LVLI"
                            | b"LVSP"
                            | b"WOOP"
                            | b"MGEF"
                    )
                })
            {
                return Err(bad("invalid record origin or temporary identity"));
            }
            let original = self
                .order
                .winner(origin)
                .filter(|rec| {
                    matches!(
                        &rec.tag.0,
                        b"NPC_"
                            | b"RACE"
                            | b"FLST"
                            | b"LVLN"
                            | b"LVLI"
                            | b"LVSP"
                            | b"WOOP"
                            | b"MGEF"
                    ) && !rec.is_deleted()
                })
                .ok_or_else(|| bad("missing record origin"))?;
            if self
                .order
                .winner(text_origin)
                .is_none_or(|record| record.tag != original.tag)
            {
                return Err(bad("record and text origins have different types"));
            }
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
            let list = if version >= 35 && r.bool()? {
                let form_flags = if r.bool()? {
                    Some((r.u32()?, r.u16()?))
                } else {
                    None
                };
                Some(ListRecordExtra {
                    form_flags,
                    section_present: r.bool()?,
                    entry_chances: r.bytes()?.to_vec(),
                })
            } else {
                None
            };
            let base_object = if version >= 37 && r.bool()? {
                let form_flags = if r.bool()? {
                    Some((r.u32()?, r.u16()?))
                } else {
                    None
                };
                Some(BaseObjectRecordExtra {
                    form_flags,
                    value_changed: r.bool()?,
                    full_name_changed: r.bool()?,
                    unnamed_flags: r.u32()?,
                })
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
                return Err(bad("record override changed editor identity"));
            }
            let localized = !plain_full
                && self
                    .order
                    .winner_ref(text_origin)
                    .is_some_and(|owner| self.order.file(owner.file).plugin.header.is_localized());
            if record.tag == T(b"NPC_") {
                if list.is_some() || base_object.is_some() {
                    return Err(bad("NPC has list metadata"));
                }
                crate::npc::Npc::from_record(&record, localized)
                    .map_err(|e| VmError::State(e.to_string()))?;
            } else if record.tag == T(b"RACE") {
                if list.is_some() || base_object.is_some() {
                    return Err(bad("race has list metadata"));
                }
                // Race overrides currently expose only WNAM. Reject other
                // fields instead of admitting opaque mutable race state.
                let unchanged = |record: &soe_esm::Record| {
                    record
                        .subrecords
                        .iter()
                        .filter(|sub| sub.tag != T(b"WNAM"))
                        .cloned()
                        .collect::<Vec<_>>()
                };
                if temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || base_object.is_some()
                    || unchanged(&record) != unchanged(original)
                    || record.all(T(b"WNAM")).count() != 1
                {
                    return Err(bad("race override changed fields outside skin"));
                }
                let bytes = record
                    .required(T(b"WNAM"))
                    .map_err(|_| bad("missing race skin"))?;
                let raw = u32::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| bad("invalid race skin length"))?,
                );
                let target = if raw == 0 {
                    None
                } else {
                    links
                        .get(&raw)
                        .copied()
                        .or_else(|| self.resolve_from(origin, raw))
                };
                if (raw != 0 && target.is_none())
                    || target.is_some_and(|id| {
                        self.order
                            .winner(id)
                            .is_none_or(|r| r.tag != T(b"ARMO") || r.is_deleted())
                    })
                    || links.keys().any(|token| *token != raw)
                {
                    return Err(bad("invalid race skin link"));
                }
            } else if matches!(record.tag, tag if tag == T(b"WOOP") || tag == T(b"MGEF")) {
                if list.is_some()
                    || base_object.is_none()
                    || temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || record != *original
                    || !links.is_empty()
                {
                    return Err(bad("invalid base-object override"));
                }
                if record.tag == T(b"MGEF") {
                    soe_esm::records::gameplay::MagicEffect::from_record(&record, localized)
                        .map_err(|e| VmError::State(e.to_string()))?;
                }
            } else {
                let tag = if record.tag == T(b"FLST") {
                    T(b"LNAM")
                } else {
                    T(b"LVLO")
                };
                let old: Vec<_> = original
                    .subrecords
                    .iter()
                    .filter(|sub| sub.tag == tag)
                    .collect();
                let new: Vec<_> = record
                    .subrecords
                    .iter()
                    .filter(|sub| sub.tag == tag)
                    .collect();
                let unchanged = |record: &soe_esm::Record| {
                    record
                        .subrecords
                        .iter()
                        .filter(|sub| {
                            sub.tag != tag && (tag == T(b"LNAM") || sub.tag != T(b"LLCT"))
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let extra = list.as_ref().ok_or_else(|| bad("list metadata missing"))?;
                if temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || base_object.is_some()
                    || unchanged(original) != unchanged(&record)
                    || !new.starts_with(&old)
                    || new
                        .iter()
                        .any(|sub| sub.data.len() != if tag == T(b"LNAM") { 4 } else { 12 })
                    || (tag == T(b"LNAM") && !extra.entry_chances.is_empty())
                    || (tag == T(b"LVLO") && extra.entry_chances.len() != new.len() - old.len())
                    || (!extra.section_present && new.len() != old.len())
                    || (tag == T(b"LVLO") && new.len() > 255)
                    || (tag == T(b"LVLO")
                        && original.find(T(b"LLCT")).is_none()
                        && record.find(T(b"LLCT")).is_some())
                    || (tag == T(b"LVLO")
                        && original.find(T(b"LLCT")).is_some()
                        && record.find(T(b"LLCT")) != Some(&[new.len() as u8][..]))
                {
                    return Err(bad("invalid saved list override"));
                }
            }
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
                list,
                base_object,
            };
            if values.insert(id, value).is_some() {
                return Err(bad("duplicate record override"));
            }
        }
        Ok(values)
    }
}

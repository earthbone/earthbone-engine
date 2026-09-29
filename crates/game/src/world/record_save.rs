// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW record overrides. Original plugins stay immutable and keep link provenance.

use std::collections::BTreeMap;

use soe_esm::{plugin::Subrecord, GlobalFormId, Slot, Tag};
use soe_papyrus::{vm::state::Writer, VmError};

use super::record_overrides::{
    raw_words, BaseObjectRecordExtra, ListRecordExtra, NavmeshRecordExtra, NpcRecordExtra,
    PackRecordExtra, RecordOverride, StaticObjectRecordExtra,
};
use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{World, T};

fn bad(reason: &str) -> VmError {
    VmError::State(format!("record override: {reason}"))
}

fn write_extra_flags(w: &mut Writer, flags: Option<(u32, u16)>) {
    w.bool(flags.is_some());
    if let Some((mask, value)) = flags {
        w.u32(mask);
        w.u16(value);
    }
}

fn read_extra_flags(r: &mut FormReader<'_, '_>) -> Result<Option<(u32, u16)>, VmError> {
    if r.bool()? {
        Ok(Some((r.u32()?, r.u16()?)))
    } else {
        Ok(None)
    }
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
                            | b"LIGH"
                            | b"STAT"
                            | b"MSTT"
                            | b"CONT"
                            | b"MISC"
                            | b"FURN"
                            | b"APPA"
                            | b"IDLM"
                            | b"WOOP"
                            | b"MGEF"
                            | b"PACK"
                            | b"NAVM"
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
                            | b"LIGH"
                            | b"STAT"
                            | b"MSTT"
                            | b"CONT"
                            | b"MISC"
                            | b"FURN"
                            | b"APPA"
                            | b"IDLM"
                            | b"WOOP"
                            | b"MGEF"
                            | b"PACK"
                            | b"NAVM"
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
            let (pack, navmesh) = (None, None);
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
                if list.is_some() || pack.is_some() || navmesh.is_some() {
                    return Err(bad("NPC has list metadata"));
                }
                crate::npc::Npc::from_record(&record, localized)
                    .map_err(|e| VmError::State(e.to_string()))?;
            } else if record.tag == T(b"RACE") {
                if list.is_some() || pack.is_some() || navmesh.is_some() {
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
            } else if matches!(&record.tag.0, b"CONT" | b"MISC" | b"FURN") {
                if list.is_some()
                    || temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || record.subrecords != original.subrecords
                {
                    return Err(bad("invalid saved static-object override"));
                }
            } else if matches!(&record.tag.0, b"LIGH" | b"STAT" | b"MSTT") {
                if list.is_some()
                    || temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || record.subrecords != original.subrecords
                {
                    return Err(bad("invalid saved base-object override"));
                }
            } else if matches!(&record.tag.0, b"APPA" | b"IDLM") {
                if list.is_some()
                    || temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || record.flags != original.flags
                    || record.subrecords != original.subrecords
                {
                    return Err(bad("invalid saved APPA or IDLM override"));
                }
            } else if matches!(&record.tag.0, b"WOOP" | b"MGEF") {
                if list.is_some()
                    || temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || record != *original
                    || !links.is_empty()
                {
                    return Err(bad("invalid saved WOOP or MGEF override"));
                }
                if record.tag == T(b"MGEF") {
                    soe_esm::records::gameplay::MagicEffect::from_record(&record, localized)
                        .map_err(|e| VmError::State(e.to_string()))?;
                }
            } else if record.tag == T(b"PACK") || record.tag == T(b"NAVM") {
                let expected_pack = record.tag == T(b"PACK");
                if temporary_for.is_some()
                    || text_origin != id
                    || plain_full
                    || original_race.is_some()
                    || body_tint.is_some()
                    || pending_form_flags.is_some()
                    || list.is_some()
                    || !links.is_empty()
                    || (expected_pack && navmesh.is_some())
                    || (!expected_pack && pack.is_some())
                    || unchanged_record(&record) != unchanged_record(original)
                    || record.editor_id() != original.editor_id()
                {
                    return Err(bad("invalid saved PACK or NAVM override"));
                }
                if expected_pack {
                    crate::package::Package::from_record(&record)
                        .map_err(|error| VmError::State(error.to_string()))?;
                } else {
                    soe_esm::records::navm::NavMesh::from_record(&record)
                        .map_err(|error| VmError::State(error.to_string()))?;
                }
            } else {
                if pack.is_some() || navmesh.is_some() {
                    return Err(bad("list record has PACK or NAVM metadata"));
                }
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
                base_object: None,
                static_object: None,
                pack,
                navmesh,
            };
            if values.insert(id, value).is_some() {
                return Err(bad("duplicate record override"));
            }
        }
        Ok(values)
    }

    pub(super) fn write_base_object_section(&self, w: &mut Writer) {
        let values: Vec<_> = self
            .record_overrides
            .iter()
            .filter_map(|(id, value)| {
                (matches!(&value.record.tag.0, b"LIGH" | b"STAT" | b"MSTT"))
                    .then(|| value.base_object.map(|extra| (*id, extra)))
                    .flatten()
            })
            .collect();
        w.len(values.len());
        for (id, extra) in values {
            write_form_id(w, id);
            w.bool(extra.form_flags.is_some());
            if let Some((flags, unknown)) = extra.form_flags {
                w.u32(flags);
                w.u16(unknown);
            }
            w.bool(extra.value_changed);
            w.bool(extra.full_name_changed);
        }
    }

    pub(super) fn read_base_object_section(
        &mut self,
        r: &mut FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        let count = r.len()?;
        if count > r.remaining() / 6 {
            return Err(bad("base-object section count exceeds remaining bytes"));
        }
        let mut values = Vec::with_capacity(count);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..count {
            let id = read_form_id(r)?;
            self.saved_record_identity(id)?;
            let form_flags = if r.bool()? {
                Some((r.u32()?, r.u16()?))
            } else {
                None
            };
            let value_changed = r.bool()?;
            let full_name_changed = r.bool()?;
            let extra = BaseObjectRecordExtra {
                form_flags,
                change_flags: u32::from(form_flags.is_some())
                    | (u32::from(value_changed) << 1)
                    | (u32::from(full_name_changed) << 2),
                value_changed,
                full_name_changed,
                unnamed_flags: 0,
            };
            let value = self
                .record_overrides
                .get(&id)
                .ok_or_else(|| bad("base-object section names a missing record override"))?;
            if !matches!(&value.record.tag.0, b"LIGH" | b"STAT" | b"MSTT")
                || value.base_object.is_some()
                || !seen.insert(id)
            {
                return Err(bad("invalid or duplicate base-object section entry"));
            }
            values.push((id, extra));
        }
        for (id, extra) in values {
            let record = self
                .record_overrides
                .get_mut(&id)
                .ok_or_else(|| bad("base-object override disappeared during load"))?;
            if let Some((mask, _)) = extra.form_flags {
                record.record.flags ^= mask;
            }
            record.base_object = Some(extra);
        }
        Ok(())
    }

    pub(super) fn write_woop_mgef_section(&self, w: &mut Writer) {
        let values: Vec<_> = self
            .record_overrides
            .iter()
            .filter_map(|(id, value)| {
                (matches!(&value.record.tag.0, b"WOOP" | b"MGEF"))
                    .then(|| value.base_object.map(|extra| (*id, extra)))
                    .flatten()
            })
            .collect();
        w.len(values.len());
        for (id, extra) in values {
            write_form_id(w, id);
            w.u32(extra.change_flags);
            w.bool(extra.form_flags.is_some());
            if let Some((flags, unknown)) = extra.form_flags {
                w.u32(flags);
                w.u16(unknown);
            }
            w.bool(extra.value_changed);
            w.bool(extra.full_name_changed);
            w.u32(extra.unnamed_flags);
        }
    }

    pub(super) fn read_woop_mgef_section(
        &mut self,
        r: &mut FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        let count = r.len()?;
        if count > r.remaining() / 15 {
            return Err(bad("WOOP/MGEF section count exceeds remaining bytes"));
        }
        let mut values = Vec::with_capacity(count);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..count {
            let id = read_form_id(r)?;
            self.saved_record_identity(id)?;
            let change_flags = r.u32()?;
            let form_flags = if r.bool()? {
                Some((r.u32()?, r.u16()?))
            } else {
                None
            };
            let value_changed = r.bool()?;
            let full_name_changed = r.bool()?;
            let unnamed_flags = r.u32()?;
            let record = self
                .record_overrides
                .get(&id)
                .ok_or_else(|| bad("WOOP/MGEF section names a missing record override"))?;
            if !matches!(&record.record.tag.0, b"WOOP" | b"MGEF")
                || record.base_object.is_some()
                || form_flags.is_some() != (change_flags & 1 != 0)
                || value_changed != (change_flags & 2 != 0)
                || full_name_changed != (change_flags & 4 != 0)
                || unnamed_flags != (change_flags & !7)
                || !seen.insert(id)
            {
                return Err(bad("invalid or duplicate WOOP/MGEF section entry"));
            }
            if record.record.tag == T(b"MGEF") {
                let localized = self
                    .order
                    .winner_ref(id)
                    .is_some_and(|r| self.order.file(r.file).plugin.header.is_localized());
                soe_esm::records::gameplay::MagicEffect::from_record(&record.record, localized)
                    .map_err(|error| bad(&error.to_string()))?;
            }
            values.push((
                id,
                BaseObjectRecordExtra {
                    form_flags,
                    change_flags,
                    value_changed,
                    full_name_changed,
                    unnamed_flags,
                },
            ));
        }
        for (id, extra) in values {
            let record = self
                .record_overrides
                .get_mut(&id)
                .ok_or_else(|| bad("WOOP/MGEF override disappeared during load"))?;
            if let Some((mask, _)) = extra.form_flags {
                record.record.flags ^= mask;
            }
            record.base_object = Some(extra);
        }
        Ok(())
    }

    pub(super) fn write_appa_idlm_section(&self, w: &mut Writer) {
        let values: Vec<_> = self
            .record_overrides
            .iter()
            .filter_map(|(id, value)| {
                (matches!(&value.record.tag.0, b"APPA" | b"IDLM"))
                    .then(|| value.base_object.map(|extra| (*id, extra)))
                    .flatten()
            })
            .collect();
        w.len(values.len());
        for (id, extra) in values {
            write_form_id(w, id);
            w.u32(extra.change_flags);
            w.bool(extra.form_flags.is_some());
            if let Some((flags, unknown)) = extra.form_flags {
                w.u32(flags);
                w.u16(unknown);
            }
            w.bool(extra.value_changed);
            w.bool(extra.full_name_changed);
        }
    }

    pub(super) fn read_appa_idlm_section(
        &mut self,
        r: &mut FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        let count = r.len()?;
        if count > r.remaining() / 11 {
            return Err(bad("APPA/IDLM section count exceeds remaining bytes"));
        }
        let flags = soe_save::base_object::flags::FORM;
        let value = soe_save::base_object::flags::VALUE;
        let full_name = soe_save::base_object::flags::FULL_NAME;
        let mut values = Vec::with_capacity(count);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..count {
            let id = read_form_id(r)?;
            self.saved_record_identity(id)?;
            let change_flags = r.u32()?;
            let form_flags = if r.bool()? {
                Some((r.u32()?, r.u16()?))
            } else {
                None
            };
            let value_changed = r.bool()?;
            let full_name_changed = r.bool()?;
            let record = self
                .record_overrides
                .get(&id)
                .ok_or_else(|| bad("APPA/IDLM section names a missing record override"))?;
            if !matches!(&record.record.tag.0, b"APPA" | b"IDLM")
                || record.base_object.is_some()
                || record.static_object.is_some()
                || form_flags.is_some() != (change_flags & flags != 0)
                || value_changed != (change_flags & value != 0)
                || full_name_changed != (change_flags & full_name != 0)
                || !seen.insert(id)
            {
                return Err(bad("invalid or duplicate APPA/IDLM section entry"));
            }
            values.push((
                id,
                BaseObjectRecordExtra {
                    form_flags,
                    change_flags,
                    value_changed,
                    full_name_changed,
                    unnamed_flags: 0,
                },
            ));
        }
        for (id, extra) in values {
            let record = self
                .record_overrides
                .get_mut(&id)
                .ok_or_else(|| bad("APPA/IDLM override disappeared during load"))?;
            if let Some((mask, _)) = extra.form_flags {
                record.record.flags ^= mask;
            }
            record.base_object = Some(extra);
        }
        Ok(())
    }

    pub(super) fn write_static_object_section(&self, w: &mut Writer) {
        let values: Vec<_> = self
            .record_overrides
            .iter()
            .filter_map(|(id, value)| value.static_object.as_ref().map(|extra| (*id, extra)))
            .collect();
        w.len(values.len());
        for (id, extra) in values {
            write_form_id(w, id);
            w.u32(extra.change_flags);
            w.bool(extra.form_flags.is_some());
            if let Some((flags, unknown)) = extra.form_flags {
                w.u32(flags);
                w.u16(unknown);
            }
            w.bool(extra.value_changed);
            w.bool(extra.full_name_changed);
            w.bytes(&extra.opaque_suffix);
        }
    }

    pub(super) fn read_static_object_section(
        &mut self,
        r: &mut FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        let count = r.len()?;
        if count > r.remaining() / 6 {
            return Err(bad("static-object section count exceeds remaining bytes"));
        }
        let mut values = Vec::with_capacity(count);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..count {
            let id = read_form_id(r)?;
            self.saved_record_identity(id)?;
            let change_flags = r.u32()?;
            let form_flags = if r.bool()? {
                Some((r.u32()?, r.u16()?))
            } else {
                None
            };
            let value_changed = r.bool()?;
            let full_name_changed = r.bool()?;
            let opaque_suffix = r.bytes()?.to_vec();
            let flags = soe_save::base_object::static_object_flags::FORM;
            let value = soe_save::base_object::static_object_flags::VALUE;
            let full_name = soe_save::base_object::static_object_flags::FULL_NAME;
            let record = self
                .record_overrides
                .get(&id)
                .ok_or_else(|| bad("static-object section names a missing record override"))?;
            if !matches!(&record.record.tag.0, b"CONT" | b"MISC" | b"FURN")
                || record.static_object.is_some()
                || record.base_object.is_some()
                || form_flags.is_some() != (change_flags & flags != 0)
                || value_changed != (change_flags & value != 0)
                || full_name_changed != (change_flags & full_name != 0)
                || !seen.insert(id)
            {
                return Err(bad("invalid or duplicate static-object section entry"));
            }
            values.push((
                id,
                StaticObjectRecordExtra {
                    change_flags,
                    form_flags,
                    value_changed,
                    full_name_changed,
                    opaque_suffix,
                },
            ));
        }
        for (id, extra) in values {
            let record = self
                .record_overrides
                .get_mut(&id)
                .ok_or_else(|| bad("static-object override disappeared during load"))?;
            if let Some((mask, _)) = extra.form_flags {
                record.record.flags ^= mask;
            }
            record.static_object = Some(extra);
        }
        Ok(())
    }
}

pub(super) fn write_pack_navmesh_section(world: &World, w: &mut Writer) {
    let entries: Vec<_> = world
        .record_overrides
        .iter()
        .filter(|(_, value)| value.pack.is_some() || value.navmesh.is_some())
        .collect();
    if entries.is_empty() {
        return;
    }
    w.len(entries.len());
    for (id, value) in entries {
        write_form_id(w, *id);
        w.bool(value.pack.is_some());
        if let Some(pack) = value.pack {
            write_extra_flags(w, pack.form_flags);
            w.bool(pack.waiting);
            w.bool(pack.never_run);
        }
        w.bool(value.navmesh.is_some());
        if let Some(navmesh) = value.navmesh {
            write_extra_flags(w, navmesh.form_flags);
            w.bool(navmesh.value_changed);
            w.bool(navmesh.full_name_changed);
        }
    }
}

pub(super) fn read_pack_navmesh_section(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..r.len()? {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(bad("duplicate PACK or NAVM section record"));
        }
        let pack = if r.bool()? {
            Some(PackRecordExtra {
                form_flags: read_extra_flags(r)?,
                waiting: r.bool()?,
                never_run: r.bool()?,
            })
        } else {
            None
        };
        let navmesh = if r.bool()? {
            Some(NavmeshRecordExtra {
                form_flags: read_extra_flags(r)?,
                value_changed: r.bool()?,
                full_name_changed: r.bool()?,
            })
        } else {
            None
        };
        let value = world
            .record_overrides
            .get_mut(&id)
            .ok_or_else(|| bad("PACK or NAVM section names a missing override"))?;
        match value.record.tag {
            tag if tag == T(b"PACK") && pack.is_some() && navmesh.is_none() => {
                value.pack = pack;
            }
            tag if tag == T(b"NAVM") && navmesh.is_some() && pack.is_none() => {
                value.navmesh = navmesh;
            }
            _ => return Err(bad("PACK or NAVM section has mismatched record metadata")),
        }
    }
    if world.record_overrides.iter().any(|(id, value)| {
        (value.record.tag == T(b"PACK") && value.pack.is_none())
            || (value.record.tag == T(b"NAVM") && value.navmesh.is_none())
            || (!seen.contains(id) && (value.pack.is_some() || value.navmesh.is_some()))
    }) {
        return Err(bad("PACK or NAVM section is incomplete"));
    }
    Ok(())
}

fn unchanged_record(record: &soe_esm::Record) -> Vec<Subrecord> {
    record.subrecords.clone()
}

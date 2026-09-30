// SPDX-License-Identifier: GPL-3.0-or-later

//! Mutable records in the shared world, layered over the immutable plugin table.
//! Mapped links use record-local relocations, leaving untouched bytes in their
//! original plugin namespace. This does not add a plugin or change load-order slots.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{plugin::Subrecord, GlobalFormId, Record, Tag};

use super::{World, T};

/// Saved NPC fields which have no lossless plugin-subrecord equivalent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NpcRecordExtra {
    pub original_race: Option<GlobalFormId>,
    pub body_tint: Option<[u8; 4]>,
    /// Preserved, but generic form-flag masks have not been applied yet.
    pub pending_form_flags: Option<(u32, u16)>,
}

/// Saved list fields that have no plugin subrecord equivalent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListRecordExtra {
    pub form_flags: Option<(u32, u16)>,
    pub section_present: bool,
    pub entry_chances: Vec<u8>,
}

/// Saved change markers that have no plugin-subrecord representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackRecordExtra {
    pub form_flags: Option<(u32, u16)>,
    pub waiting: bool,
    pub never_run: bool,
}

/// Saved NAVM change markers that have no plugin-subrecord representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NavmeshRecordExtra {
    pub form_flags: Option<(u32, u16)>,
    pub value_changed: bool,
    pub full_name_changed: bool,
}

/// Plugin-relative identity for a record retained while its plugin is absent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnresolvedRecordId {
    /// Original plugin filename, compared without ASCII case.
    pub plugin: String,
    /// Whether the identity came from the light-plugin list.
    pub light: bool,
    /// Local object number in the plugin's regular or light identity space.
    pub object: u32,
}

/// Opaque source data retained without binding it into the live form table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRecord {
    /// Stable plugin-relative identity of the unavailable record.
    pub id: UnresolvedRecordId,
    /// Original change-form or record payload bytes.
    pub payload: Vec<u8>,
    /// Original plugin-relative identities referenced by the payload.
    pub links: Vec<UnresolvedRecordId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordOverride {
    pub origin: GlobalFormId,
    pub text_origin: GlobalFormId,
    pub temporary_for: Option<GlobalFormId>,
    pub record: Record,
    pub links: BTreeMap<u32, GlobalFormId>,
    pub plain_full: bool,
    pub npc: NpcRecordExtra,
    pub list: Option<ListRecordExtra>,
    pub pack: Option<PackRecordExtra>,
    pub navmesh: Option<NavmeshRecordExtra>,
}

pub(super) fn raw_words(record: &Record) -> BTreeSet<u32> {
    // Include unaligned words so untouched opaque subrecords cannot collide.
    record
        .subrecords
        .iter()
        .flat_map(|sub| sub.data.windows(4))
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .chain([0])
        .collect()
}

impl RecordOverride {
    pub fn new(origin: GlobalFormId, record: Record) -> Self {
        Self {
            origin,
            text_origin: origin,
            temporary_for: None,
            record,
            links: BTreeMap::new(),
            plain_full: false,
            npc: NpcRecordExtra::default(),
            list: None,
            pack: None,
            navmesh: None,
        }
    }

    pub fn replace(&mut self, tag: Tag, values: impl IntoIterator<Item = Vec<u8>>) {
        let at = self
            .record
            .subrecords
            .iter()
            .position(|sub| sub.tag == tag)
            .unwrap_or(self.record.subrecords.len());
        self.record.subrecords.retain(|sub| sub.tag != tag);
        self.record.subrecords.splice(
            at..at,
            values.into_iter().map(|data| Subrecord { tag, data }),
        );
    }

    pub fn set(&mut self, tag: Tag, data: impl Into<Vec<u8>>) {
        self.replace(tag, [data.into()]);
    }

    pub fn link(
        &mut self,
        used: &mut BTreeSet<u32>,
        id: Option<GlobalFormId>,
    ) -> Result<u32, String> {
        let Some(id) = id else {
            return Ok(0);
        };
        if let Some((&raw, _)) = self.links.iter().find(|(_, value)| **value == id) {
            return Ok(raw);
        }
        let mut raw = u32::MAX;
        while used.contains(&raw) {
            raw = raw.checked_sub(1).ok_or("record link space exhausted")?;
        }
        used.insert(raw);
        self.links.insert(raw, id);
        Ok(raw)
    }
}

impl World {
    /// Retain opaque data for a record whose plugin is not available to the live
    /// load order. The data is kept separately from resolved forms.
    ///
    /// # Errors
    /// Empty plugin names, invalid regular or light object numbers, or a
    /// duplicate identity.
    pub fn retain_unresolved_record(&mut self, mut record: UnresolvedRecord) -> Result<(), String> {
        normalize_unresolved_id(&mut record.id)?;
        for link in &mut record.links {
            normalize_unresolved_id(link)?;
        }
        if self.unresolved_records.contains_key(&record.id) {
            return Err("unresolved record identity already exists".into());
        }
        self.unresolved_records.insert(record.id.clone(), record);
        Ok(())
    }

    /// Return opaque records retained for plugins absent from the live table.
    #[must_use]
    pub fn unresolved_records(&self) -> &BTreeMap<UnresolvedRecordId, UnresolvedRecord> {
        &self.unresolved_records
    }

    pub(super) fn record_text_localized(&self, id: GlobalFormId, tag: Tag) -> bool {
        if tag == T(b"FULL") && self.record_overrides.get(&id).is_some_and(|r| r.plain_full) {
            return false;
        }
        let origin = self.record_overrides.get(&id).map_or(id, |r| r.text_origin);
        self.order
            .winner_ref(origin)
            .is_some_and(|r| self.order.file(r.file).plugin.header.is_localized())
    }

    pub fn restored_npc_extra(&self, id: GlobalFormId) -> Option<&NpcRecordExtra> {
        self.record_overrides.get(&id).map(|record| &record.npc)
    }

    pub fn restored_list_extra(&self, id: GlobalFormId) -> Option<&ListRecordExtra> {
        self.record_overrides.get(&id)?.list.as_ref()
    }

    /// Saved PACK markers used by package consumers.
    pub fn restored_pack_extra(&self, id: GlobalFormId) -> Option<PackRecordExtra> {
        self.record_overrides.get(&id)?.pack
    }

    /// Saved NAVM markers used by navmesh consumers.
    pub fn restored_navmesh_extra(&self, id: GlobalFormId) -> Option<NavmeshRecordExtra> {
        self.record_overrides.get(&id)?.navmesh
    }
}

pub(super) fn normalize_unresolved_id(id: &mut UnresolvedRecordId) -> Result<(), String> {
    id.plugin = id.plugin.to_ascii_lowercase();
    if id.plugin.is_empty() || id.plugin.contains(['/', '\\', '\0']) {
        return Err("invalid unresolved plugin name".into());
    }
    let limit = if id.light { 0xFFF } else { 0xFF_FFFF };
    if id.object == 0 || id.object > limit {
        return Err("unresolved object number is outside its identity space".into());
    }
    Ok(())
}

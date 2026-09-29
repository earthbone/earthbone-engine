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

/// Saved runtime fields whose owning record has no plugin-subrecord equivalent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogueRecordExtra {
    pub form_flags: Option<(u32, u16)>,
    pub info_said_once: Option<bool>,
    pub smqn_last_run_times: Option<Vec<u32>>,
    pub scene_active: Option<bool>,
    pub scene_active_data: Option<Vec<u8>>,
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
    pub saved_form_flags: Option<(u32, u16)>,
    pub dialogue: Option<DialogueRecordExtra>,
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
            saved_form_flags: None,
            dialogue: None,
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

    /// Saved INFO said-once state, when a save supplied an explicit marker.
    pub fn restored_info_said_once(&self, id: GlobalFormId) -> Option<bool> {
        self.record_overrides
            .get(&id)?
            .dialogue
            .as_ref()?
            .info_said_once
    }

    /// The unmodified six-byte saved change-form flags for a record.
    pub fn restored_dialogue_form_flags(&self, id: GlobalFormId) -> Option<(u32, u16)> {
        self.record_overrides
            .get(&id)?
            .dialogue
            .as_ref()?
            .form_flags
    }

    /// Saved SMQN last-run values, retaining each value's exact float bits.
    pub fn restored_smqn_last_run_times(&self, id: GlobalFormId) -> Option<&[u32]> {
        self.record_overrides
            .get(&id)?
            .dialogue
            .as_ref()?
            .smqn_last_run_times
            .as_deref()
    }

    /// Opaque runtime bytes from an active saved SCEN record.
    pub fn restored_scene_active_data(&self, id: GlobalFormId) -> Option<&[u8]> {
        self.record_overrides
            .get(&id)?
            .dialogue
            .as_ref()?
            .scene_active_data
            .as_deref()
    }
}

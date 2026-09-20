// SPDX-License-Identifier: GPL-3.0-or-later

//! NPC change application uses xEdit's NPC_ subrecords and CommonLib TESNPC
//! bodyTintColor/originalRace. Link identities are mapped before record mutation.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_save::{npc::NpcChange, RefId, Save};

use super::record_overrides::{raw_words, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct NpcRestoreReport {
    pub bases: usize,
    pub missing_links: BTreeSet<GlobalFormId>,
    pub pending_form_flags: BTreeMap<GlobalFormId, (u32, u16)>,
}

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::Npc {
        id,
        reason: reason.into(),
    }
}

struct Builder<'a, 's> {
    id: GlobalFormId,
    value: RecordOverride,
    used: BTreeSet<u32>,
    mapping: &'a SavedFormMap<'s>,
}

impl Builder<'_, '_> {
    fn link(&mut self, saved: RefId) -> Result<u32, RestoreError> {
        let id = self.mapping.reference(saved)?;
        self.value
            .link(&mut self.used, id)
            .map_err(|e| invalid(self.id, e))
    }

    fn apply(&mut self, npc: &NpcChange) -> Result<(), RestoreError> {
        self.value.npc.pending_form_flags = npc.form_flags;
        for (tag, data) in [
            (T(b"ACBS"), npc.base.as_ref().map(|v| v.as_slice())),
            (T(b"AIDT"), npc.ai.as_ref().map(|v| v.as_slice())),
            (T(b"DNAM"), npc.skills.as_ref().map(|v| v.as_slice())),
        ] {
            if let Some(data) = data {
                self.value.set(tag, data);
            }
        }
        if let Some(name) = &npc.name {
            let mut bytes = name.bytes().to_vec();
            if bytes.last() != Some(&0) {
                bytes.push(0);
            }
            self.value.set(T(b"FULL"), bytes);
            self.value.plain_full = true;
        }
        if let Some(factions) = &npc.factions {
            let mut values = vec![];
            for entry in &factions.entries {
                let mut data = self.link(entry.form)?.to_le_bytes().to_vec();
                data.extend([entry.rank as u8, 0, 0, 0]);
                values.push(data);
            }
            self.value.replace(T(b"SNAM"), values);
        }
        if let Some(spells) = &npc.spells {
            let mut values = vec![];
            for id in spells
                .spells
                .entries
                .iter()
                .chain(&spells.leveled_spells.entries)
                .chain(&spells.shouts.entries)
            {
                values.push(self.link(*id)?.to_le_bytes().to_vec());
            }
            let count =
                u32::try_from(values.len()).map_err(|_| invalid(self.id, "too many NPC spells"))?;
            self.value.replace(T(b"SPLO"), values);
            self.value.set(T(b"SPCT"), count.to_le_bytes());
        }
        for (tag, saved) in [
            (T(b"CNAM"), npc.class),
            (T(b"RNAM"), npc.race.map(|(new, _)| new)),
            (T(b"DOFT"), npc.default_outfit),
            (T(b"SOFT"), npc.sleep_outfit),
        ] {
            if let Some(saved) = saved {
                let raw = self.link(saved)?;
                self.value.set(tag, raw.to_le_bytes());
            }
        }
        if let Some((_, old)) = npc.race {
            self.value.npc.original_race = self.mapping.reference(old)?;
        }
        if let Some(gender) = npc.gender {
            if gender > 1 {
                return Err(invalid(self.id, "invalid NPC gender"));
            }
            let mut config = self
                .value
                .record
                .find(T(b"ACBS"))
                .filter(|v| v.len() == 24)
                .ok_or_else(|| invalid(self.id, "NPC gender requires ACBS"))?
                .to_vec();
            config[0] = (config[0] & !1) | gender;
            self.value.set(T(b"ACBS"), config);
        }
        if let Some(face) = &npc.face {
            if let Some(head) = &face.head {
                let hair = self.link(head.hair_color)?;
                let texture = self.link(head.texture_set)?;
                self.value.set(T(b"HCLF"), hair.to_le_bytes());
                self.value.set(T(b"FTST"), texture.to_le_bytes());
                self.value.npc.body_tint = Some(head.skin_color);
                let color: Vec<_> = head.skin_color[..3]
                    .iter()
                    .flat_map(|v| (f32::from(*v) / 255.0).to_le_bytes())
                    .collect();
                self.value.set(T(b"QNAM"), color);
            } else {
                for tag in [T(b"HCLF"), T(b"FTST"), T(b"QNAM")] {
                    self.value.replace(tag, []);
                }
            }
            let mut parts = vec![];
            for part in &face.head_parts.entries {
                parts.push(self.link(*part)?.to_le_bytes().to_vec());
            }
            self.value.replace(T(b"PNAM"), parts);
            if let Some(morph) = &face.morph {
                self.value.set(
                    T(b"NAM9"),
                    morph
                        .options
                        .iter()
                        .flat_map(|v| v.to_le_bytes())
                        .collect::<Vec<_>>(),
                );
                self.value.set(
                    T(b"NAMA"),
                    morph
                        .presets
                        .iter()
                        .flat_map(|v| v.to_le_bytes())
                        .collect::<Vec<_>>(),
                );
            } else {
                self.value.replace(T(b"NAM9"), []);
                self.value.replace(T(b"NAMA"), []);
            }
        }
        Ok(())
    }
}

impl World {
    pub(super) fn apply_saved_npc(
        &self,
        id: GlobalFormId,
        value: RecordOverride,
        npc: &NpcChange,
        mapping: &SavedFormMap<'_>,
    ) -> Result<RecordOverride, RestoreError> {
        let mut builder = Builder {
            id,
            used: raw_words(&value.record),
            value,
            mapping,
        };
        builder.apply(npc)?;
        let mut value = builder.value;
        let present = raw_words(&value.record);
        value.links.retain(|raw, _| present.contains(raw));
        let localized =
            !value.plain_full && self.record_text_localized(value.text_origin, T(b"FULL"));
        crate::npc::Npc::from_record(&value.record, localized)
            .map_err(|e| invalid(id, e.to_string()))?;
        Ok(value)
    }

    /// Apply saved NPC records before constructing references or actors.
    /// Every record is prepared before any override becomes visible. No events run.
    ///
    /// # Errors
    /// Existing dependent state, invalid mapping, malformed data, missing/deleted
    /// bases, duplicate NPC changes or an invalid resulting record.
    pub fn restore_vanilla_npcs(&mut self, save: &Save) -> Result<NpcRestoreReport, RestoreError> {
        if !self.refs.is_empty()
            || !self.actors.is_empty()
            || !self.actor_bases.is_empty()
            || !self.actor_templates.is_empty()
            || !self.record_overrides.is_empty()
        {
            return Err(invalid(
                GlobalFormId { file: 0, object: 7 },
                "restore NPCs before references and actors",
            ));
        }
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        let mut report = NpcRestoreReport::default();
        for change in &save.change_forms {
            let Some(npc) = change.npc_change()? else {
                continue;
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let record = self
                .order
                .winner(id)
                .filter(|r| r.tag == T(b"NPC_") && !r.is_deleted())
                .ok_or_else(|| invalid(id, "missing, deleted or non-NPC base"))?;
            let value =
                self.apply_saved_npc(id, RecordOverride::new(id, record.clone()), &npc, &mapping)?;
            for link in value.links.values().copied().chain(value.npc.original_race) {
                if self.record(link).is_none_or(|r| r.is_deleted()) {
                    report.missing_links.insert(link);
                }
            }
            if let Some(flags) = value.npc.pending_form_flags {
                report.pending_form_flags.insert(id, flags);
            }
            if prepared.insert(id, value).is_some() {
                return Err(invalid(id, "duplicate NPC change"));
            }
        }
        report.bases = prepared.len();
        self.record_overrides = prepared;
        Ok(report)
    }
}

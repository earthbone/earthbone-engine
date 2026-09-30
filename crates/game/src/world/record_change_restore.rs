// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved CLAS, FACT and RELA changes follow xEdit's
//! `wbDefinitionsTES5Saves.pas` and `wbDefinitionsTES5.pas` record layouts.

use std::collections::BTreeMap;

use soe_esm::{GlobalFormId, Record};
use soe_save::{RefKind, Save};

use super::record_overrides::{raw_words, RecordOverride};
use super::{World, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::RecordChange {
        id,
        reason: reason.into(),
    }
}

fn write_word(
    record: &mut RecordOverride,
    used: &mut std::collections::BTreeSet<u32>,
    at: usize,
    id: Option<GlobalFormId>,
) -> Result<(), String> {
    let raw = record.link(used, id)?;
    let data = record.record.find(T(b"DATA")).unwrap_or(&[]).to_vec();
    let mut data = data;
    if data.len() < 16 {
        data.resize(16, 0);
    }
    data[at..at + 4].copy_from_slice(&raw.to_le_bytes());
    record.set(T(b"DATA"), data);
    Ok(())
}

impl World {
    /// Read the relationship rank stored for an actor pair in a RELA record.
    ///
    /// # Errors
    /// This lookup is infallible and returns `None` when no matching record
    /// exists or its data is too short to contain the pair and rank.
    #[must_use]
    pub fn relationship_record_rank(
        &self,
        parent: GlobalFormId,
        child: GlobalFormId,
    ) -> Option<i32> {
        let ids = self.order.winners_of(T(b"RELA")).map(|(id, _)| id).chain(
            self.record_overrides
                .iter()
                .filter(|(id, value)| value.record.tag == T(b"RELA") && !self.order.contains(**id))
                .map(|(id, _)| *id),
        );
        for id in ids {
            let Some(data) = self.record(id).and_then(|record| record.find(T(b"DATA"))) else {
                continue;
            };
            if data.len() < 10 {
                continue;
            }
            let saved_parent = u32::from_le_bytes(data[0..4].try_into().ok()?);
            let saved_child = u32::from_le_bytes(data[4..8].try_into().ok()?);
            if self.resolve_from(id, saved_parent) == Some(parent)
                && self.resolve_from(id, saved_child) == Some(child)
            {
                let rank = u16::from_le_bytes(data[8..10].try_into().ok()?);
                let signed = i32::from(rank as i16);
                return Some(if signed < 0 { signed } else { 4 - signed });
            }
        }
        None
    }

    /// Apply decoded CLAS, FACT and RELA changes to live records atomically.
    ///
    /// # Errors
    /// Rejects malformed changes, unresolved or mistyped records, duplicate
    /// changes, invalid links and link-space collisions before changing state.
    pub fn restore_vanilla_record_changes(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut prepared = BTreeMap::new();
        let mut faction_updates = BTreeMap::new();

        for change in &save.change_forms {
            let expected = match change.record_type() {
                "CLAS" => T(b"CLAS"),
                "FACT" => T(b"FACT"),
                "RELA" => T(b"RELA"),
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let synthetic_relationship = expected == T(b"RELA")
                && change.form.kind() == RefKind::Created
                && id.file == super::DYNAMIC_FILE;
            let record = if synthetic_relationship {
                if self.record_overrides.contains_key(&id) {
                    return Err(invalid(id, "duplicate or already overridden record"));
                }
                None
            } else {
                Some(
                    self.order
                        .winner(id)
                        .filter(|record| record.tag == expected && !record.is_deleted())
                        .ok_or_else(|| invalid(id, "missing, deleted or wrong record type"))?,
                )
            };
            if prepared.contains_key(&id) || self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "duplicate or already overridden record"));
            }

            let synthetic_record;
            let record = if let Some(record) = record {
                record
            } else {
                synthetic_record = Record::new(T(b"RELA"), 0xFF00_0000 | id.object);
                &synthetic_record
            };
            let mut value = RecordOverride::new(id, record.clone());
            let mut used = raw_words(record);
            match change.record_type() {
                "CLAS" => {
                    // xEdit defines CLAS form flags and a null tag-skills marker.
                    let decoded = change
                        .class_change()?
                        .ok_or_else(|| invalid(id, "missing CLAS change"))?;
                    value.saved_form_flags =
                        decoded.form_flags.map(|flags| (flags.flags, flags.unknown));
                    if let Some((flags, _)) = value.saved_form_flags {
                        value.record.flags = flags;
                    }
                }
                "FACT" => {
                    let decoded = change
                        .faction_change()?
                        .ok_or_else(|| invalid(id, "missing FACT change"))?;
                    value.saved_form_flags =
                        decoded.form_flags.map(|flags| (flags.flags, flags.unknown));
                    if let Some((flags, _)) = value.saved_form_flags {
                        value.record.flags = flags;
                    }
                    let mut cached_reactions = None;
                    let mut cached_tracks_crime = None;
                    if let Some(flags) = decoded.faction_flags {
                        value.set(T(b"DATA"), flags.to_le_bytes());
                        cached_tracks_crime = Some(flags & (1 << 6) != 0);
                    }
                    if let Some(reactions) = decoded.reactions {
                        let mut subrecords = Vec::with_capacity(reactions.entries.len());
                        let mut state = BTreeMap::new();
                        for entry in reactions.entries {
                            let target = mapping.reference(entry.faction)?;
                            let raw = value
                                .link(&mut used, target)
                                .map_err(|reason| invalid(id, reason))?;
                            let mut data = Vec::with_capacity(12);
                            data.extend(raw.to_le_bytes());
                            data.extend(entry.modifier.to_le_bytes());
                            data.extend(entry.reaction.to_le_bytes());
                            subrecords.push(data);
                            if let Some(target) = target {
                                state.insert(target, entry.reaction as i32);
                            }
                        }
                        value.replace(T(b"XNAM"), subrecords);
                        cached_reactions = Some(state);
                    }
                    if let Some(crime) = decoded.crime_counts {
                        let mut bytes = Vec::with_capacity(16);
                        for word in crime {
                            bytes.extend(word.to_le_bytes());
                        }
                        value.set(T(b"CRVA"), bytes);
                    }
                    faction_updates.insert(id, (cached_reactions, cached_tracks_crime));
                }
                "RELA" => {
                    let decoded = change
                        .relationship_change()?
                        .ok_or_else(|| invalid(id, "missing RELA change"))?;
                    value.saved_form_flags =
                        decoded.form_flags.map(|flags| (flags.flags, flags.unknown));
                    if let Some((flags, _)) = value.saved_form_flags {
                        value.record.flags = flags;
                    }
                    if let Some(created) = decoded.created {
                        let parent = mapping.reference(created[0])?;
                        let child = mapping.reference(created[1])?;
                        let association = mapping.reference(created[2])?;
                        write_word(&mut value, &mut used, 0, parent)
                            .map_err(|reason| invalid(id, reason))?;
                        write_word(&mut value, &mut used, 4, child)
                            .map_err(|reason| invalid(id, reason))?;
                        write_word(&mut value, &mut used, 12, association)
                            .map_err(|reason| invalid(id, reason))?;
                    }
                    if let Some(rank) = decoded.rank {
                        let mut data = value.record.find(T(b"DATA")).unwrap_or(&[]).to_vec();
                        if data.len() < 16 {
                            data.resize(16, 0);
                        }
                        data[8..10].copy_from_slice(&(rank as u16).to_le_bytes());
                        value.set(T(b"DATA"), data);
                    }
                }
                _ => unreachable!(),
            }
            let present = raw_words(&value.record);
            value.links.retain(|raw, _| present.contains(raw));
            prepared.insert(id, value);
        }

        let count = prepared.len();
        self.record_overrides.extend(prepared);
        for (id, (reactions, tracks_crime)) in faction_updates {
            if let Some(faction) = self.factions.get_mut(&id) {
                if let Some(reactions) = reactions {
                    faction.reactions = reactions;
                }
                if let Some(tracks_crime) = tracks_crime {
                    faction.tracks_crime = tracks_crime;
                }
            }
        }
        Ok(count)
    }
}

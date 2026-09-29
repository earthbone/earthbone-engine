// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic record patch passes over a resolved Skyrim load order.
//!
//! The winning-record and override rules follow Mutagen's Winning Overrides
//! and Create, Duplicate, and Override documentation. Record storage and
//! resolution use this crate's raw plugin model.

use crate::error::{Error, Result};
use crate::load_order::{GlobalFormId, LoadOrder};
use crate::plugin::{is_form_id_field, Entry, GroupKind, Plugin, Record};
use crate::tag::Tag;

/// A patching pass over winning top-level STAT records.
///
/// The callback sees winners in resolved FormID order. Changed records are
/// written as overrides in the output plugin. Unchanged records are omitted.
pub struct PatcherSession<'a> {
    load_order: &'a LoadOrder,
    output: Plugin,
}

impl<'a> PatcherSession<'a> {
    /// Create a patching pass using an output plugin with loaded masters.
    ///
    /// # Errors
    /// Returns an error when an output master is absent from the load order.
    pub fn new(load_order: &'a LoadOrder, output: Plugin) -> Result<Self> {
        for master in &output.header.masters {
            if load_order.position_of(master).is_none() {
                return Err(Error::Invalid(format!(
                    "output plugin master {master} is not loaded"
                )));
            }
        }
        Ok(Self { load_order, output })
    }

    /// Apply a callback to each winning top-level STAT record.
    ///
    /// The callback edits a working copy. Only records that differ from the
    /// current winner are added to the output. Existing output overrides are
    /// reused, so multiple patcher passes can combine edits.
    ///
    /// # Errors
    /// Returns an error when a changed record is nested or its owning plugin
    /// is not listed as an output master.
    pub fn patch_winning_stat_records(
        &mut self,
        mut patch: impl FnMut(GlobalFormId, &mut Record),
    ) -> Result<usize> {
        let mut changed = 0;
        for (form, record_ref) in self.load_order.winners() {
            let source = self.load_order.record(record_ref);
            if source.tag != Tag(*b"STAT") {
                continue;
            }
            let mut candidate = self
                .output_record(form)
                .cloned()
                .unwrap_or_else(|| source.clone());
            let original = candidate.clone();
            patch(form, &mut candidate);
            if candidate == original {
                continue;
            }
            if source
                .subrecords
                .iter()
                .chain(&candidate.subrecords)
                .any(|subrecord| {
                    is_form_id_field(source.tag, subrecord.tag) || subrecord.tag == Tag(*b"VMAD")
                })
            {
                return Err(Error::Invalid(format!(
                    "cannot patch STAT form {form} with FormID links before link relocation"
                )));
            }
            if !is_top_level(
                &self.load_order.file(record_ref.file).plugin,
                record_ref.record,
            ) {
                return Err(Error::Invalid(format!(
                    "cannot patch nested STAT form {form} before group relocation"
                )));
            }
            let source_file = self.load_order.file(record_ref.file);
            let source_masters = &source_file.plugin.header.masters;
            let compatible_masters = self.output.header.masters.len() > source_masters.len()
                && source_masters
                    .iter()
                    .zip(&self.output.header.masters)
                    .all(|(source, output)| source.eq_ignore_ascii_case(output))
                && self.output.header.masters[source_masters.len()]
                    .eq_ignore_ascii_case(&source_file.name);
            if !compatible_masters {
                return Err(Error::Invalid(format!(
                    "output masters must preserve {}'s master order and place it at index {} for form {form}",
                    source_file.name,
                    source_masters.len()
                )));
            }
            let owner = self
                .load_order
                .files()
                .get(form.file as usize)
                .ok_or_else(|| Error::Invalid(format!("form owner {form} is not loaded")))?;
            let master_index = self
                .output
                .header
                .masters
                .iter()
                .position(|name| owner.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "output plugin does not list {} as a master for form {form}",
                        owner.name
                    ))
                })?;
            candidate.form_id = ((master_index as u32) << 24) | form.object;
            if let Some(index) = self.output_record_index(form) {
                self.output.records[index] = candidate;
            } else {
                self.output.push_top(candidate);
            }
            changed += 1;
        }
        self.sync_record_count();
        Ok(changed)
    }

    /// Borrow the plugin produced by the patching pass.
    pub fn output(&self) -> &Plugin {
        &self.output
    }

    /// Consume the patching pass and return its output plugin.
    pub fn into_output(self) -> Plugin {
        self.output
    }

    fn output_record_index(&self, form: GlobalFormId) -> Option<usize> {
        let owner = self.load_order.files().get(form.file as usize)?;
        let master_index = self
            .output
            .header
            .masters
            .iter()
            .position(|name| owner.name.eq_ignore_ascii_case(name))?;
        let raw = ((master_index as u32) << 24) | form.object;
        self.output
            .records
            .iter()
            .position(|record| record.form_id == raw)
    }

    fn output_record(&self, form: GlobalFormId) -> Option<&Record> {
        self.output_record_index(form)
            .and_then(|index| self.output.records.get(index))
    }

    fn sync_record_count(&mut self) {
        let count = self.output.records.len() as u32;
        self.output.header.record_count = count;
        if let Some(hedr) = self
            .output
            .header_record
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.tag == Tag(*b"HEDR"))
        {
            if hedr.data.len() >= 8 {
                hedr.data[4..8].copy_from_slice(&count.to_le_bytes());
            }
        }
    }
}

fn is_top_level(plugin: &Plugin, record_index: usize) -> bool {
    plugin.groups.iter().any(|group| {
        matches!(group.kind, GroupKind::Top(_))
            && group
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Record(index) if *index == record_index))
    })
}

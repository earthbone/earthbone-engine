// SPDX-License-Identifier: GPL-3.0-or-later

//! What xEdit's Quick Auto Clean would change in each plugin, counted, not
//! changed.
//!
//! The vanilla engine crashes on a deleted reference or navmesh that another
//! record or a script still names, and an override that repeats its master
//! byte for byte silently wins over every plugin loaded between the two.
//! xEdit repairs all three in the file. This engine reads the file as it is:
//! a deleted placed object resolves and starts disabled (`CellIndex` keeps
//! it under `deleted_references`), a deleted navmesh drops out of the graph
//! with its neighbours' links cut, and an identical override is ordinary
//! data. The report exists so an author can see what a file carries, and so
//! the live tests can compare the counts with LOOT's masterlist notes.
//!
//! An override counts as identical to master when its record flags (the
//! compressed bit aside) and its subrecord list equal those of the record it
//! overrides in the highest of its own masters that carries the id. xEdit
//! compares element by element under its conflict rules and skips fields
//! it marks as ignorable, so its count is at or above this byte-level one.

use crate::cell_index::PLACED_TAGS;
use crate::load_order::{GlobalFormId, LoadOrder};
use crate::plugin::{flags, Record};
use crate::tag;

/// One plugin's counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDirt {
    pub name: String,
    /// Overrides equal to the record they override.
    pub identical_to_master: usize,
    /// Placed objects (`REFR`, `ACHR` and the projectile types) with the
    /// deleted flag.
    pub deleted_references: usize,
    pub deleted_navmeshes: usize,
    /// Any other record with the deleted flag.
    pub deleted_other: usize,
}

impl PluginDirt {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.identical_to_master == 0
            && self.deleted_references == 0
            && self.deleted_navmeshes == 0
            && self.deleted_other == 0
    }
}

/// The counts for every file of a load order, in load order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirtyReport {
    pub plugins: Vec<PluginDirt>,
}

impl DirtyReport {
    #[must_use]
    pub fn plugin(&self, name: &str) -> Option<&PluginDirt> {
        self.plugins
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn identical_to_master(&self) -> usize {
        let mut count = 0;
        for plugin in &self.plugins {
            count += plugin.identical_to_master;
        }
        count
    }

    #[must_use]
    pub fn deleted_references(&self) -> usize {
        let mut total = 0;
        for plugin in &self.plugins {
            total += plugin.deleted_references;
        }
        total
    }

    #[must_use]
    pub fn deleted_navmeshes(&self) -> usize {
        self.plugins
            .iter()
            .fold(0, |count, plugin| count + plugin.deleted_navmeshes)
    }
}

fn identical(a: &Record, b: &Record) -> bool {
    a.tag == b.tag
        && (a.flags & !flags::COMPRESSED) == (b.flags & !flags::COMPRESSED)
        && a.subrecords == b.subrecords
}

impl LoadOrder {
    /// Count, per file, what xEdit's cleaning would touch. One pass over
    /// every record of every file.
    #[must_use]
    pub fn dirty_report(&self) -> DirtyReport {
        let _span = soe_trace::Span::new("esm.dirty_report", "esm");
        let mut plugins = Vec::with_capacity(self.files().len());
        for (file_index, file) in self.files().iter().enumerate() {
            let mut dirt = PluginDirt {
                name: file.name.clone(),
                ..Default::default()
            };
            for rec in &file.plugin.records {
                if rec.is_deleted() {
                    if PLACED_TAGS.contains(&rec.tag) {
                        dirt.deleted_references += 1;
                    } else if rec.tag == tag::NAVM {
                        dirt.deleted_navmeshes += 1;
                    } else {
                        dirt.deleted_other += 1;
                    }
                    continue;
                }
                let id = self.resolve(file_index, rec.form_id);
                if id.file as usize == file_index {
                    continue;
                }
                if let Some(master) = self.master_record(file_index, id) {
                    if identical(rec, master) {
                        dirt.identical_to_master += 1;
                    }
                }
            }
            plugins.push(dirt);
        }
        DirtyReport { plugins }
    }

    /// The record `file_index` overrides for `id`: the one in the highest of
    /// the file's masters that carries the id, or failing that the override
    /// just below the file in load order.
    fn master_record(&self, file_index: usize, id: GlobalFormId) -> Option<&Record> {
        let masters = &self.file(file_index).master_positions;
        let below = || {
            self.overrides(id)
                .iter()
                .rev()
                .filter(|r| r.file < file_index)
        };
        let from_master = below().find(|r| masters.contains(&r.file));
        from_master
            .or_else(|| below().next())
            .map(|r| self.record(*r))
    }
}

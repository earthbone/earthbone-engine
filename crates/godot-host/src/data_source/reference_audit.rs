// SPDX-License-Identifier: GPL-3.0-or-later

//! Placed-reference accounting using CellIndex and xEdit REFR flags/XESP.
//! Counts describe a prepared cell, never an authoritative enable-state cache.

use std::collections::{BTreeMap, HashSet};

use soe_esm::{tag, Tag};

use super::{models, DataSource, GlobalFormId, LoadedCell, Refr};

/// Disjoint outcomes for indexed placed references of one winning base type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceCounts {
    pub placed: usize,
    /// References with at least one visible surface, counted once per FormID.
    pub built: usize,
    pub initially_disabled: usize,
    pub enable_parent: usize,
    pub runtime_disabled: usize,
    pub markers: usize,
    /// Explicit reasons for outcomes outside the placed-mesh path.
    pub other: BTreeMap<&'static str, usize>,
}

impl DataSource {
    /// Audit every indexed reference against returned geometry and live World state.
    /// An enabled reference not yet prepared is reported as deferred, never built.
    /// Deleted winners are included even though CellIndex stores them separately.
    #[must_use]
    pub fn reference_counts(
        &self,
        cell: GlobalFormId,
        loaded: &LoadedCell,
        world: Option<&soe_game::World>,
    ) -> BTreeMap<Tag, ReferenceCounts> {
        let drawn: HashSet<_> = loaded
            .surfaces
            .iter()
            .map(|surface| surface.reference)
            .collect();
        let index = self.cell_index();
        let mut result: BTreeMap<Tag, ReferenceCounts> = BTreeMap::new();
        for &id in index
            .references(cell)
            .iter()
            .chain(index.deleted_references(cell))
        {
            let owner = self.order.winner_ref(id);
            let record = owner.map(|owner| self.order.record(owner));
            let reference = record
                .filter(|record| record.tag == tag::REFR)
                .and_then(|record| Refr::from_record(record).ok());
            let raw_base = record
                .and_then(|record| record.find(Tag::new(b"NAME")))
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .map(u32::from_le_bytes);
            let base = owner
                .zip(raw_base)
                .and_then(|(owner, raw)| self.order.winner(self.order.resolve(owner.file, raw)));
            let counts = result
                .entry(base.map_or(Tag::new(b"????"), |base| base.tag))
                .or_default();
            counts.placed += 1;
            let Some(record) = record else {
                *counts.other.entry("missing reference").or_default() += 1;
                continue;
            };
            if record.is_deleted() {
                *counts.other.entry("deleted").or_default() += 1;
                continue;
            }
            let authored = soe_game::World::authored_reference_enabled(&self.order, id);
            if !world.map_or(authored, |world| world.reference_enabled(id)) {
                if world.is_some() && authored {
                    counts.runtime_disabled += 1;
                } else if record.find(Tag::new(b"XESP")).is_some() {
                    counts.enable_parent += 1;
                } else {
                    counts.initially_disabled += 1;
                }
                continue;
            }
            if models::is_editor_reference(record)
                || base.is_some_and(models::is_marker_base)
                || loaded.stats.reference_skips.get(&id) == Some(&"marker model")
            {
                counts.markers += 1;
                continue;
            }
            if drawn.contains(&id) {
                counts.built += 1;
                continue;
            }
            let reason = if record.tag != tag::REFR {
                "actor presentation"
            } else if reference.is_none() {
                "malformed reference"
            } else if base.is_none() {
                "missing base"
            } else if let Some(reason) = loaded.stats.reference_skips.get(&id) {
                reason
            } else if base.is_some_and(|base| models::render_model_for(base).is_some()) {
                "deferred"
            } else {
                "no placed model"
            };
            *counts.other.entry(reason).or_default() += 1;
        }
        result
    }
}

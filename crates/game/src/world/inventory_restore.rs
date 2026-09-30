// SPDX-License-Identifier: GPL-3.0-or-later

//! Atomic inventory pass after reference placement and actor-template restoration.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::{World, DYNAMIC_FILE, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct InventoryRestoreReport {
    pub references: usize,
    pub entries: usize,
    pub stacks: usize,
    /// Mapped item identities with no current record. Created bases need a later pass.
    pub missing_items: BTreeSet<GlobalFormId>,
}

fn bad(id: GlobalFormId, reason: impl std::fmt::Display) -> RestoreError {
    RestoreError::WorldReference {
        id,
        reason: reason.to_string(),
    }
}

impl World {
    /// Replace inventories using authored quantities and saved per-item deltas.
    /// All decoding and validation completes before any reference is changed.
    /// This does not queue item events or select new leveled actor templates.
    ///
    /// # Errors
    /// Missing references/bases/template selections, malformed data or quantities.
    pub fn restore_vanilla_inventories(
        &mut self,
        save: &Save,
    ) -> Result<InventoryRestoreReport, RestoreError> {
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut created_bases = BTreeSet::new();
        if let Some(objects) = save.created_objects()? {
            for object in objects
                .weapon_enchantments
                .iter()
                .chain(&objects.armor_enchantments)
                .chain(&objects.potions)
                .chain(&objects.poisons)
            {
                if let Some(id) = mapping.reference(object.form)? {
                    if id.file == DYNAMIC_FILE {
                        created_bases.insert(id);
                    }
                }
            }
        }
        let mut prepared = BTreeMap::new();
        let mut report = InventoryRestoreReport::default();
        for change in &save.change_forms {
            let Some(contents) = change.reference_contents(save.form_version)? else {
                continue;
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let state = self
                .refs
                .get(&id)
                .ok_or_else(|| bad(id, "restore reference placements first"))?;
            let base = state
                .base
                .ok_or_else(|| bad(id, "missing reference base"))?;
            if self.order.winner(base).is_none() && !created_bases.contains(&base) {
                return Err(bad(id, "restore created base records first"));
            }
            if self.tag_of(base) == Some(T(b"NPC_")) {
                self.reference_component_source(id, base, crate::npc::template::INVENTORY)
                    .map_err(|error| bad(id, error))?;
            }
            let authored = self.reference_inventory(Some(id), base);
            let inventory = match contents.inventory {
                Some(saved) => authored
                    .with_vanilla_changes(&saved, &mapping)
                    .map_err(|error| bad(id, error))?,
                None => authored,
            };
            report.entries += inventory.len();
            for (item, _) in inventory.iter() {
                report.stacks += inventory.stacks(*item).len();
                if self.order.winner(*item).is_none() {
                    report.missing_items.insert(*item);
                }
            }
            if prepared.insert(id, inventory).is_some() {
                return Err(bad(id, "duplicate inventory reference"));
            }
        }
        report.references = prepared.len();
        for (id, inventory) in prepared {
            if let Some(state) = self.refs.get_mut(&id) {
                state.inventory = inventory;
            }
        }
        Ok(report)
    }
}

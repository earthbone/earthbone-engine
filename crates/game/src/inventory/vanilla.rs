// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply saved deltas to authored contents. CommonLib TESObjectREFR::GetInventory
//! excludes authored counts for entries carrying ExtraLeveledItem.

use std::collections::BTreeSet;

use soe_save::reference::{ExtraData, Field as SavedField, Inventory as SavedInventory};

use super::{Extra, Field, Inventory, Stack};
use crate::vanilla_save::{RestoreError, SavedFormMap};

fn bad(reason: impl std::fmt::Display) -> RestoreError {
    RestoreError::Inventory(reason.to_string())
}

fn map_fields(
    fields: &[SavedField],
    mapping: &SavedFormMap<'_>,
    depth: usize,
) -> Result<Vec<Field>, RestoreError> {
    if depth > 128 {
        return Err(bad("extra-data nesting exceeds 128 levels"));
    }
    fields
        .iter()
        .map(|field| {
            Ok(match field {
                SavedField::Byte(v) => Field::Byte(*v),
                SavedField::Short(v) => Field::Short(*v),
                SavedField::Word(v) => Field::Word(*v),
                SavedField::Float(v) => Field::FloatBits(v.to_bits()),
                SavedField::Reference(v) => Field::Form(mapping.reference(*v)?),
                SavedField::String(v) => Field::Text(v.bytes().to_vec()),
                SavedField::Bytes(v) => Field::Bytes(v.clone()),
                SavedField::Array(v) => Field::Array(
                    v.entries
                        .iter()
                        .map(|row| map_fields(row, mapping, depth + 1))
                        .collect::<Result<_, _>>()?,
                ),
                SavedField::Group(v) => Field::Group(map_fields(v, mapping, depth + 1)?),
            })
        })
        .collect()
}

fn map_stack(saved: &ExtraData, mapping: &SavedFormMap<'_>) -> Result<Stack, RestoreError> {
    let mut count = None;
    let mut extras = Vec::new();
    for extra in &saved.entries {
        if extra.kind == 36 {
            let [SavedField::Short(value)] = extra.fields.as_slice() else {
                return Err(bad("invalid stack count field"));
            };
            if count.replace(i32::from(*value as i16)).is_some() {
                return Err(bad("duplicate stack count"));
            }
        } else {
            extras.push(Extra {
                kind: extra.kind,
                fields: map_fields(&extra.fields, mapping, 0)?,
            });
        }
    }
    let count = count.unwrap_or(1);
    let kinds: Vec<_> = extras.iter().map(|extra| extra.kind).collect();
    Stack::new(count, extras)
        .map_err(|error| bad(format!("stack count={count}, extras={kinds:?}: {error}")))
}

impl Inventory {
    /// Apply saved deltas and distinct stacks to these authored contents.
    /// The caller supplies authored quantities, not an already-restored inventory.
    ///
    /// # Errors
    /// Invalid mappings, duplicate item entries, invalid stack fields or quantities.
    pub fn with_vanilla_changes(
        &self,
        saved: &SavedInventory,
        mapping: &SavedFormMap<'_>,
    ) -> Result<Self, RestoreError> {
        let mut next = self.clone();
        let mut seen = BTreeSet::new();
        for entry in &saved.entries {
            let item = mapping
                .reference(entry.item)?
                .ok_or_else(|| bad("null inventory item"))?;
            if !seen.insert(item) {
                return Err(bad("duplicate inventory item"));
            }
            let leveled = entry
                .stacks
                .iter()
                .any(|stack| stack.entries.iter().any(|extra| extra.kind == 46));
            let base = if leveled { 0 } else { self.count(item) };
            let total = base
                .checked_add(entry.delta)
                .ok_or_else(|| bad("inventory delta overflow"))?;
            let stacks = entry
                .stacks
                .iter()
                .map(|stack| {
                    map_stack(stack, mapping).map_err(|error| {
                        bad(format!(
                            "item {item}, authored={base}, delta={}, total={total}: {error}",
                            entry.delta
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            next.restore(item, total, stacks).map_err(|error| {
                bad(format!(
                    "item {item}: authored={base}, delta={}, total={total}: {error}",
                    entry.delta
                ))
            })?;
        }
        Ok(next)
    }
}

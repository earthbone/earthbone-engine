// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact selection over CommonLib's count/extra-list model. Crafting patches use
//! ExtraHealth, ExtraCharge, ExtraPoison, ExtraEnchantment and ExtraSoul save
//! field layouts from CommonLibSSE-NG.

use std::sync::Arc;

use soe_esm::GlobalFormId;

use super::{Bundle, Error, Extra, Field, Inventory, Stack};

/// Choose the undescribed remainder or an index in `Inventory::stacks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackSource {
    Plain,
    Extra(usize),
}

/// A transient selection, valid until this inventory changes or is restored.
/// A cloned inventory accepts it for preparing an atomic crafting transaction.
#[derive(Debug, Clone)]
pub struct Selection {
    item: GlobalFormId,
    source: StackSource,
    count: i32,
    revision: Arc<()>,
}

impl Selection {
    pub fn item(&self) -> GlobalFormId {
        self.item
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    /// The stack source selected from the inventory.
    pub fn source(&self) -> StackSource {
        self.source
    }
}

fn validate_patch(extra: &Extra) -> Result<(), Error> {
    let valid = match (extra.kind, extra.fields.as_slice()) {
        (37 | 40, [Field::FloatBits(bits)]) => {
            let value = f32::from_bits(*bits);
            value.is_finite() && value >= 0.0
        }
        (62, [Field::Form(Some(_)), Field::Word(count)]) => *count > 0,
        (155, [Field::Form(Some(_)), Field::Short(_)]) => true,
        (156, [Field::Byte(level)]) => (1..=5).contains(level),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidExtra(extra.kind))
    }
}

impl Inventory {
    /// Select an exact quantity without consuming it or changing metadata.
    ///
    /// # Errors
    /// Missing items, invalid indices or quantities, bookkeeping or overlapping lists.
    pub fn select(
        &self,
        item: GlobalFormId,
        source: StackSource,
        count: i32,
    ) -> Result<Selection, Error> {
        let entry = self.entries.get(&item).ok_or(Error::Selection)?;
        let represented: i64 = entry.stacks.iter().map(|s| i64::from(s.count.max(0))).sum();
        if represented > i64::from(entry.total) {
            return Err(Error::AmbiguousStacks);
        }
        let available = match source {
            StackSource::Plain => entry.total - represented as i32,
            StackSource::Extra(index) => entry.stacks.get(index).ok_or(Error::Selection)?.count,
        };
        if count <= 0 || count > available {
            return Err(Error::Selection);
        }
        Ok(Selection {
            item,
            source,
            count,
            revision: Arc::clone(&self.revision),
        })
    }

    /// Select one item from the undescribed remainder or first available stack.
    ///
    /// # Errors
    /// Missing items, ambiguous overlapping stack counts or insufficient quantity.
    pub fn select_any(&self, item: GlobalFormId, count: i32) -> Result<Selection, Error> {
        match self.select(item, StackSource::Plain, count) {
            Ok(selected) => return Ok(selected),
            Err(Error::AmbiguousStacks) => return Err(Error::AmbiguousStacks),
            Err(_) => {}
        }
        let entry = self.entries.get(&item).ok_or(Error::Selection)?;
        for (index, stack) in entry.stacks.iter().enumerate() {
            if stack.count >= count {
                if let Ok(selected) = self.select(item, StackSource::Extra(index), count) {
                    return Ok(selected);
                }
            }
        }
        Err(Error::Selection)
    }

    /// Inspect the selected metadata. Plain items return `None`.
    ///
    /// # Errors
    /// The selection belongs to a different inventory revision.
    pub fn selected_stack(&self, selected: &Selection) -> Result<Option<&Stack>, Error> {
        self.check_selection(selected)?;
        Ok(match selected.source {
            StackSource::Plain => None,
            StackSource::Extra(index) => self
                .entries
                .get(&selected.item)
                .and_then(|e| e.stacks.get(index)),
        })
    }

    fn check_selection(&self, selected: &Selection) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.revision, &selected.revision) {
            return Err(Error::StaleSelection);
        }
        self.select(selected.item, selected.source, selected.count)?;
        Ok(())
    }

    /// Patch quality (37), charge (40), poison (62) or enchantment (155).
    /// Unmentioned extras retain their exact bits. A partial selection splits off
    /// a new stack and returns a fresh selection of only the changed items.
    ///
    /// # Errors
    /// Stale selection, duplicate patch kinds or malformed/unsupported patch fields.
    pub fn update_selected(
        &mut self,
        selected: &Selection,
        changes: Vec<Extra>,
    ) -> Result<Selection, Error> {
        self.check_selection(selected)?;
        let patch = Stack::new(selected.count, changes)?;
        for extra in &patch.extras {
            validate_patch(extra)?;
        }
        let mut extras = self
            .selected_stack(selected)?
            .map_or_else(Vec::new, |s| s.extras.clone());
        for extra in patch.extras {
            if let Some(existing) = extras.iter_mut().find(|e| e.kind == extra.kind) {
                *existing = extra;
            } else {
                extras.push(extra);
            }
        }
        let replacement = Stack::new(selected.count, extras)?;
        let entry = self
            .entries
            .get_mut(&selected.item)
            .ok_or(Error::Selection)?;
        let index = match selected.source {
            StackSource::Extra(index) if entry.stacks[index].count == selected.count => {
                entry.stacks[index] = replacement;
                index
            }
            source => {
                if let StackSource::Extra(index) = source {
                    entry.stacks[index].count -= selected.count;
                }
                let index = entry.stacks.len();
                entry.stacks.push(replacement);
                index
            }
        };
        self.revision = Arc::new(());
        Ok(Selection {
            item: selected.item,
            source: StackSource::Extra(index),
            count: selected.count,
            revision: Arc::clone(&self.revision),
        })
    }

    /// Consume exactly the selected items, preserving their complete extra data.
    ///
    /// # Errors
    /// The selection is stale or belongs to another inventory.
    pub fn take_selected(&mut self, selected: &Selection) -> Result<Bundle, Error> {
        self.check_selection(selected)?;
        let entry = self
            .entries
            .get_mut(&selected.item)
            .ok_or(Error::Selection)?;
        let mut stacks = Vec::new();
        if let StackSource::Extra(index) = selected.source {
            let mut part = entry.stacks[index].clone();
            part.count = selected.count;
            stacks.push(part);
            entry.stacks[index].count -= selected.count;
            if entry.stacks[index].count == 0 {
                entry.stacks.remove(index);
            }
        }
        entry.total -= selected.count;
        if entry.total == 0 && entry.stacks.is_empty() {
            self.entries.remove(&selected.item);
        }
        self.revision = Arc::new(());
        Ok(Bundle {
            total: selected.count,
            stacks,
        })
    }
}

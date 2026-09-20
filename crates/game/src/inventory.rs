// SPDX-License-Identifier: GPL-3.0-or-later

//! Inventory quantities and distinct stacks in the shared reference state.
//! CommonLib InventoryEntryData/ExtraDataList supply the count/extra distinction.
//! Saved RefIDs become resolved forms before entering this model.

mod selection;
mod vanilla;

pub use selection::{Selection, StackSource};

use std::collections::BTreeMap;
use std::sync::Arc;

use soe_esm::GlobalFormId;

/// Extra-data fields with mapped identities and exact floating-point bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Byte(u8),
    Short(u16),
    Word(u32),
    FloatBits(u32),
    Form(Option<GlobalFormId>),
    Text(Vec<u8>),
    Bytes(Vec<u8>),
    Array(Vec<Vec<Field>>),
    Group(Vec<Field>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extra {
    pub kind: u8,
    pub fields: Vec<Field>,
}

/// Items sharing one extra-data list, or a nonpositive leveled bookkeeping entry.
/// Count is stored only here. Bookkeeping entries are not transferable items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    count: i32,
    extras: Vec<Extra>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid or overflowing inventory quantity")]
    Quantity,
    #[error("extra-data count must be represented by the stack quantity")]
    ExtraCount,
    #[error("duplicate extra-data type {0}")]
    DuplicateExtra(u8),
    #[error("item extra-data nesting exceeds 128 levels")]
    Nesting,
    #[error("inventory selection is stale or belongs to another inventory")]
    StaleSelection,
    #[error("stack does not contain the requested quantity")]
    Selection,
    #[error("overlapping stack counts cannot identify an exact item")]
    AmbiguousStacks,
    #[error("invalid crafted item extra-data type or fields: {0}")]
    InvalidExtra(u8),
    #[error("inventory holder or recipient is missing or invalid")]
    Holder,
    #[error("inventory base form or extra-data form is missing or invalid")]
    Form,
}

fn validate_fields(fields: &[Field], depth: usize) -> Result<(), Error> {
    if depth > 128 {
        return Err(Error::Nesting);
    }
    for field in fields {
        match field {
            Field::Array(rows) => {
                for row in rows {
                    validate_fields(row, depth + 1)?;
                }
            }
            Field::Group(fields) => validate_fields(fields, depth + 1)?,
            _ => {}
        }
    }
    Ok(())
}

impl Stack {
    /// Build a stack, rejecting duplicate counts and malformed quantities.
    ///
    /// # Errors
    /// Nonpositive count without leveled metadata, duplicate extras, count extra or deep nesting.
    pub fn new(count: i32, extras: Vec<Extra>) -> Result<Self, Error> {
        if count <= 0 && !extras.iter().any(|extra| extra.kind == 46) {
            return Err(Error::Quantity);
        }
        let mut seen = [false; 256];
        for extra in &extras {
            if extra.kind == 36 {
                return Err(Error::ExtraCount);
            }
            if std::mem::replace(&mut seen[usize::from(extra.kind)], true) {
                return Err(Error::DuplicateExtra(extra.kind));
            }
            validate_fields(&extra.fields, 0)?;
        }
        Ok(Self { count, extras })
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    pub fn extras(&self) -> &[Extra] {
        &self.extras
    }

    pub fn extra(&self, kind: u8) -> Option<&Extra> {
        self.extras.iter().find(|extra| extra.kind == kind)
    }

    pub fn worn(&self) -> bool {
        self.extra(22).is_some() || self.extra(23).is_some()
    }
}

/// One authoritative item total and its ordered stack descriptions.
/// Saved extra-list counts can overlap and include leveled bookkeeping.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bundle {
    total: i32,
    stacks: Vec<Stack>,
}

impl Bundle {
    pub fn count(&self) -> i32 {
        self.total
    }

    pub fn stacks(&self) -> &[Stack] {
        &self.stacks
    }

    pub(crate) fn unequip(&mut self) {
        for stack in &mut self.stacks {
            stack.extras.retain(|extra| !matches!(extra.kind, 22 | 23));
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Inventory {
    entries: BTreeMap<GlobalFormId, Bundle>,
    // CONTEXT: Clones can prepare transactions, but any mutation invalidates selections.
    revision: Arc<()>,
}

impl Inventory {
    pub fn count(&self, item: GlobalFormId) -> i32 {
        self.entries.get(&item).map_or(0, Bundle::count)
    }

    pub fn stacks(&self, item: GlobalFormId) -> &[Stack] {
        self.entries.get(&item).map_or(&[], |entry| &entry.stacks)
    }

    pub fn contains(&self, item: GlobalFormId) -> bool {
        self.entries.contains_key(&item)
    }

    pub(crate) fn unequip(&mut self, item: GlobalFormId) {
        if let Some(entry) = self.entries.get_mut(&item) {
            entry.unequip();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.revision = Arc::new(());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&GlobalFormId, i32)> {
        self.entries.iter().map(|(id, entry)| (id, entry.count()))
    }

    /// Restore an item total and its ordered stack descriptions.
    ///
    /// # Errors
    /// A negative item total. Extra-list counts do not have to partition the total.
    pub fn restore(
        &mut self,
        item: GlobalFormId,
        total: i32,
        stacks: Vec<Stack>,
    ) -> Result<(), Error> {
        if total < 0 {
            return Err(Error::Quantity);
        }
        self.entries.insert(item, Bundle { total, stacks });
        self.revision = Arc::new(());
        Ok(())
    }

    /// Add plain items without changing the existing distinct stacks.
    ///
    /// # Errors
    /// Negative count or a total exceeding the signed quantity range.
    pub fn add(&mut self, item: GlobalFormId, count: i32) -> Result<(), Error> {
        if count < 0 || self.count(item).checked_add(count).is_none() {
            return Err(Error::Quantity);
        }
        if count != 0 {
            self.entries.entry(item).or_default().total += count;
            self.revision = Arc::new(());
        }
        Ok(())
    }

    /// Remove plain items first, then distinct stacks in their saved order.
    /// Partial stacks retain their metadata on both sides of the split.
    pub fn take(&mut self, item: GlobalFormId, count: i32) -> Bundle {
        let mut remaining = count.max(0).min(self.count(item));
        if remaining == 0 {
            return Bundle::default();
        }
        let Some(entry) = self.entries.get_mut(&item) else {
            return Bundle::default();
        };
        let represented: i64 = entry
            .stacks
            .iter()
            .map(|stack| i64::from(stack.count.max(0)))
            .sum();
        let plain = (i64::from(entry.total) - represented).max(0) as i32;
        let mut taken = Bundle {
            total: remaining,
            stacks: vec![],
        };
        entry.total -= remaining;
        remaining -= remaining.min(plain);
        for stack in &mut entry.stacks {
            if remaining == 0 {
                break;
            }
            if stack.count <= 0 {
                continue;
            }
            let n = remaining.min(stack.count);
            let mut part = stack.clone();
            part.count = n;
            taken.stacks.push(part);
            stack.count -= n;
            remaining -= n;
        }
        entry
            .stacks
            .retain(|stack| stack.count != 0 || stack.extra(46).is_some());
        if entry.count() == 0 && entry.stacks.is_empty() {
            self.entries.remove(&item);
        }
        self.revision = Arc::new(());
        taken
    }

    /// Receive a bundle without flattening distinct item data.
    ///
    /// # Errors
    /// The resulting total exceeds the signed quantity range.
    pub fn receive(&mut self, item: GlobalFormId, bundle: Bundle) -> Result<(), Error> {
        let count = bundle.count();
        self.count(item).checked_add(count).ok_or(Error::Quantity)?;
        if count != 0 {
            let entry = self.entries.entry(item).or_default();
            entry.total += bundle.total;
            entry.stacks.extend(bundle.stacks);
            self.revision = Arc::new(());
        }
        Ok(())
    }
}

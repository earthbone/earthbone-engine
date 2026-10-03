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
use soe_esm::LoadOrder;

use soe_commonlib_shim::extra::{
    ExtraDataSource, ExtraDataType, ExtraEnchantment, ExtraHotkey, ExtraLeveledCreature,
};
use soe_commonlib_shim::form::FormId;

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
    pub(crate) fn plain_one() -> Self {
        Self {
            count: 1,
            extras: Vec::new(),
        }
    }

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

    /// Borrow the live stack through the source-level CommonLib extra-data API.
    pub fn commonlib_extra_data<'a>(&'a self, order: &'a LoadOrder) -> StackExtraData<'a> {
        StackExtraData { stack: self, order }
    }

    /// The concrete item identity from CommonLib's ExtraOriginalReference.
    /// ExtraReferenceHandle names a container and is not an item identity.
    pub fn reference(&self) -> Option<GlobalFormId> {
        match self.extra(32)?.fields.as_slice() {
            [Field::Form(reference)] => *reference,
            _ => None,
        }
    }

    pub(crate) fn with_reference(mut self, reference: GlobalFormId) -> Self {
        self.extras.retain(|extra| extra.kind != 32);
        self.extras.push(Extra {
            kind: 32,
            fields: vec![Field::Form(Some(reference))],
        });
        self
    }

    pub(crate) fn set_owner(&mut self, owner: Option<GlobalFormId>) {
        self.extras.retain(|extra| extra.kind != 33);
        if let Some(owner) = owner {
            self.extras.push(Extra {
                kind: 33,
                fields: vec![Field::Form(Some(owner))],
            });
        }
    }

    pub(crate) fn set_display_name(&mut self, name: &str) {
        self.extras.retain(|extra| extra.kind != 153);
        self.extras.push(Extra {
            kind: 153,
            fields: vec![
                Field::Form(None),
                Field::Form(None),
                Field::Word(0xffff_fffe),
                Field::Text(name.as_bytes().to_vec()),
            ],
        });
    }

    pub fn worn(&self) -> bool {
        self.extra(22).is_some() || self.extra(23).is_some()
    }

    /// Remove one extra-data kind after it has been fully consumed, such as
    /// a weapon poison coating spent on a successful hit. A no-op if the
    /// stack does not carry that kind.
    pub(crate) fn clear_extra(&mut self, kind: u8) {
        self.extras.retain(|extra| extra.kind != kind);
    }
}

pub struct StackExtraData<'a> {
    stack: &'a Stack,
    order: &'a LoadOrder,
}

impl ExtraDataSource for StackExtraData<'_> {
    fn has_type(&self, kind: ExtraDataType) -> bool {
        self.stack.extra(kind as u8).is_some()
    }

    fn enchantment(&self) -> Option<ExtraEnchantment> {
        let [Field::Form(Some(id)), Field::Short(charge)] =
            self.stack.extra(0x9B)?.fields.as_slice()
        else {
            return None;
        };
        Some(ExtraEnchantment {
            enchantment: FormId(self.order.runtime_id(*id)),
            charge: *charge,
            remove_on_unequip: None,
        })
    }

    fn hotkey(&self) -> Option<ExtraHotkey> {
        let [Field::Byte(slot)] = self.stack.extra(0x49)?.fields.as_slice() else {
            return None;
        };
        Some(ExtraHotkey {
            slot: (*slot <= 7).then_some(*slot),
        })
    }

    fn leveled_creature(&self) -> Option<ExtraLeveledCreature> {
        let [Field::Form(Some(original)), Field::Form(Some(template))] =
            self.stack.extra(0x2D)?.fields.as_slice()
        else {
            return None;
        };
        Some(ExtraLeveledCreature {
            original_base: FormId(self.order.runtime_id(*original)),
            template_base: FormId(self.order.runtime_id(*template)),
        })
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
    pub(crate) fn from_stack(stack: Stack) -> Self {
        Self {
            total: stack.count,
            stacks: vec![stack],
        }
    }

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

    /// Quantity carried by a particular original reference, excluding other copies.
    pub fn reference_count(&self, reference: GlobalFormId) -> i32 {
        self.entries.values().fold(0_i32, |total, entry| {
            let count = entry
                .stacks
                .iter()
                .filter(|stack| stack.reference() == Some(reference))
                .fold(0_i32, |sum, stack| sum.saturating_add(stack.count.max(0)));
            total.saturating_add(count.min(entry.total))
        })
    }

    pub(crate) fn find_reference(&self, reference: GlobalFormId) -> Option<(GlobalFormId, &Stack)> {
        self.entries.iter().find_map(|(&item, entry)| {
            entry
                .stacks
                .iter()
                .find(|stack| {
                    stack.reference() == Some(reference)
                        && stack.count > 0
                        && stack.count <= entry.total
                })
                .map(|stack| (item, stack))
        })
    }

    pub(crate) fn take_reference(
        &mut self,
        reference: GlobalFormId,
    ) -> Option<(GlobalFormId, Bundle)> {
        let (item, _) = self.find_reference(reference)?;
        let entry = self.entries.get_mut(&item)?;
        let index = entry
            .stacks
            .iter()
            .position(|stack| stack.reference() == Some(reference))?;
        let stack = entry.stacks.remove(index);
        entry.total -= stack.count;
        let bundle = Bundle::from_stack(stack);
        if entry.total == 0 && entry.stacks.is_empty() {
            self.entries.remove(&item);
        }
        Some((item, bundle))
    }

    /// Remove stacks selected by metadata while leaving undescribed quantity
    /// and all other stacks in this inventory.
    pub(crate) fn take_stacks_where(
        &mut self,
        item: GlobalFormId,
        mut predicate: impl FnMut(&Stack) -> bool,
    ) -> Bundle {
        let Some(entry) = self.entries.get_mut(&item) else {
            return Bundle::default();
        };
        let represented: i64 = entry
            .stacks
            .iter()
            .map(|stack| i64::from(stack.count.max(0)))
            .sum();
        if represented > i64::from(entry.total) {
            return Bundle::default();
        }
        let mut selected = Vec::new();
        let mut remaining = Vec::new();
        for stack in std::mem::take(&mut entry.stacks) {
            if stack.count > 0 && predicate(&stack) {
                selected.push(stack);
            } else {
                remaining.push(stack);
            }
        }
        let total = selected
            .iter()
            .fold(0_i32, |count, stack| count.saturating_add(stack.count));
        entry.total -= total;
        entry.stacks = remaining;
        if entry.total == 0 && entry.stacks.is_empty() {
            self.entries.remove(&item);
        }
        if total == 0 {
            return Bundle::default();
        }
        self.revision = Arc::new(());
        Bundle {
            total,
            stacks: selected,
        }
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

    /// Add selected positive items, combining descriptions with identical extra data.
    pub(crate) fn add_selected_stack(
        &mut self,
        item: GlobalFormId,
        stack: Stack,
    ) -> Result<(), Error> {
        if stack.count <= 0 {
            return Err(Error::Quantity);
        }
        let total = self
            .count(item)
            .checked_add(stack.count)
            .ok_or(Error::Quantity)?;
        let entry = self.entries.entry(item).or_default();
        if let Some(existing) = entry
            .stacks
            .iter_mut()
            .find(|entry| entry.extras == stack.extras)
        {
            existing.count = existing
                .count
                .checked_add(stack.count)
                .ok_or(Error::Quantity)?;
        } else {
            entry.stacks.push(stack);
        }
        entry.total = total;
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
            // CONTEXT: Splitting quantity cannot clone a concrete reference identity.
            if n < stack.count {
                part.extras.retain(|extra| extra.kind != 32);
            }
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

    /// Clear one extra-data kind from exactly the stack at `index`, leaving
    /// its count and every other stack of this item untouched. Used to
    /// consume a spent weapon poison coating without disturbing unrelated
    /// instances that share the same base item.
    ///
    /// # Errors
    /// Missing item or an out-of-range stack index.
    pub(crate) fn clear_stack_extra(
        &mut self,
        item: GlobalFormId,
        index: usize,
        kind: u8,
    ) -> Result<(), Error> {
        let entry = self.entries.get_mut(&item).ok_or(Error::Selection)?;
        let stack = entry.stacks.get_mut(index).ok_or(Error::Selection)?;
        stack.clear_extra(kind);
        self.revision = Arc::new(());
        Ok(())
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

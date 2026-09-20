// SPDX-License-Identifier: GPL-3.0-or-later

//! Inventory notification contracts from CK OnItemAdded/OnItemRemoved and
//! AddInventoryEventFilter. Filters are per recipient and apply when queued.

use super::{Alloc, Handle, Pending, World};
use crate::inventory::{Error, Extra, Field, Selection, StackSource};
use soe_esm::GlobalFormId;
use soe_papyrus::Value;

impl World {
    /// Select a quantity from one stack or the plain remainder of a holder's items.
    ///
    /// # Errors
    /// Invalid holder, base form, quantity, stack index or ambiguous saved lists.
    pub fn select_item(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        source: StackSource,
        count: i32,
    ) -> Result<Selection, Error> {
        if !self.is_inventory_base(item) {
            return Err(Error::Form);
        }
        self.reference(holder)
            .ok_or(Error::Holder)?
            .inventory
            .select(item, source, count)
    }

    /// Patch selected instance extras and return the updated selection.
    /// Quantity does not change, so this emits no inventory count event.
    ///
    /// # Errors
    /// Invalid holder, stale selection, malformed patches or invalid linked forms.
    pub fn update_item(
        &mut self,
        holder: GlobalFormId,
        selected: &Selection,
        changes: Vec<Extra>,
    ) -> Result<Selection, Error> {
        if !self.is_inventory_base(selected.item()) {
            return Err(Error::Form);
        }
        for extra in &changes {
            let tag = match extra.kind {
                155 => Some(*b"ENCH"),
                62 => Some(*b"ALCH"),
                _ => None,
            };
            if let Some(tag) = tag {
                let Some(Field::Form(Some(form))) = extra.fields.first() else {
                    return Err(Error::InvalidExtra(extra.kind));
                };
                if self
                    .record(*form)
                    .is_none_or(|r| r.is_deleted() || r.tag.as_bytes() != &tag)
                {
                    return Err(Error::Form);
                }
            }
        }
        self.reference(holder)
            .ok_or(Error::Holder)?
            .inventory
            .update_selected(selected, changes)
    }

    /// Consume or transfer the exact selected items and queue count events.
    /// Received stacks lose worn markers, as in the ordinary transfer path.
    ///
    /// # Errors
    /// Invalid holders, self-transfer, stale selection or recipient overflow.
    pub fn remove_selected_notified(
        &mut self,
        holder: GlobalFormId,
        selected: &Selection,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<i32, Error> {
        if target == Some(holder) {
            return Err(Error::Holder);
        }
        let mut source = self
            .reference(holder)
            .ok_or(Error::Holder)?
            .inventory
            .clone();
        let mut bundle = source.take_selected(selected)?;
        let recipient = if let Some(target) = target {
            let mut inventory = self
                .reference(target)
                .ok_or(Error::Holder)?
                .inventory
                .clone();
            bundle.unequip();
            inventory.receive(selected.item(), bundle)?;
            Some((target, inventory))
        } else {
            None
        };
        // CONTEXT: Both inventories are prepared before either live state is changed.
        self.reference(holder).ok_or(Error::Holder)?.inventory = source;
        if let Some((target, inventory)) = recipient {
            self.reference(target).ok_or(Error::Holder)?.inventory = inventory;
        }
        self.inventory_event(holder, selected.item(), -selected.count(), target, alloc);
        if let Some(target) = target {
            self.inventory_event(
                target,
                selected.item(),
                selected.count(),
                Some(holder),
                alloc,
            );
        }
        Ok(selected.count())
    }

    /// Base forms supported by inventory stacks, from xEdit's item lists.
    pub(crate) fn is_inventory_base(&self, item: GlobalFormId) -> bool {
        self.record(item).is_some_and(|record| {
            !record.is_deleted()
                && matches!(
                    record.tag.as_bytes(),
                    b"ALCH"
                        | b"AMMO"
                        | b"ARMO"
                        | b"BOOK"
                        | b"INGR"
                        | b"KEYM"
                        | b"LIGH"
                        | b"MISC"
                        | b"SCRL"
                        | b"SLGM"
                        | b"WEAP"
                )
        })
    }

    /// Queue a count change for the reference and its running quest aliases.
    /// Positive delta means added. A consumed/created stack has no item reference.
    pub fn inventory_event(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        delta: i32,
        other: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) {
        let Some(count) = delta.checked_abs().filter(|count| *count > 0) else {
            return;
        };
        let direct = self.object_of(Handle::Form(holder), alloc);
        let player_alias = (self.player == Some(holder)).then_some(GlobalFormId {
            file: 0,
            object: super::PLAYER_OBJECT,
        });
        let mut aliases: Vec<_> = self
            .quests
            .iter()
            .filter(|(_, state)| state.running)
            .flat_map(|(&quest, state)| {
                state.aliases.iter().filter_map(move |(&id, &reference)| {
                    reference
                        .is_some_and(|reference| {
                            reference == holder || Some(reference) == player_alias
                        })
                        .then_some((quest, id))
                })
            })
            .collect();
        aliases.sort_unstable();
        let mut recipients = vec![direct];
        for (quest, id) in aliases {
            recipients.push(self.object_of(Handle::Alias { quest, id }, alloc));
        }
        let item_value = self.form_value(Some(item), alloc);
        let other_value = self.form_value(other, alloc);
        let name = if delta > 0 {
            "OnItemAdded"
        } else {
            "OnItemRemoved"
        };
        for object in recipients {
            let filters = self
                .inventory_filters
                .get(&object)
                .cloned()
                .unwrap_or_default();
            let mut allowed = filters.is_empty();
            for filter in filters {
                if filter == item
                    || self
                        .form_list(filter)
                        .is_some_and(|forms| forms.contains(&item))
                {
                    allowed = true;
                    break;
                }
            }
            if allowed {
                self.pending.push(Pending::Event {
                    object,
                    name: name.into(),
                    args: vec![
                        item_value.clone(),
                        Value::Int(count),
                        Value::None,
                        other_value.clone(),
                    ],
                });
            }
        }
    }

    /// Add a stack through the script-facing path, with checked quantity and events.
    pub fn add_item_notified(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        alloc: Alloc<'_>,
    ) -> bool {
        if count <= 0 || !self.is_inventory_base(item) {
            return false;
        }
        let Some(reference) = self.reference(holder) else {
            return false;
        };
        if reference.inventory.add(item, count).is_err() {
            return false;
        }
        self.inventory_event(holder, item, count, None, alloc);
        true
    }

    /// Remove or transfer a stack, then notify both holders. No partial overflow transfer.
    pub fn remove_item_notified(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> i32 {
        let moved = count.min(self.item_count(holder, item)).max(0);
        if moved == 0 || target == Some(holder) {
            return 0;
        }
        if let Some(target) = target {
            let Some(reference) = self.reference(target) else {
                return 0;
            };
            if reference.inventory.count(item).checked_add(moved).is_none() {
                return 0;
            }
        }
        let moved = self.remove_item(holder, item, moved, target);
        if moved == 0 {
            return 0;
        }
        self.inventory_event(holder, item, -moved, target, alloc);
        if let Some(target) = target {
            self.inventory_event(target, item, moved, Some(holder), alloc);
        }
        moved
    }
}

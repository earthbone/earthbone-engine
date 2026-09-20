// SPDX-License-Identifier: GPL-3.0-or-later

//! Inventory notification contracts from CK OnItemAdded/OnItemRemoved and
//! AddInventoryEventFilter. Filters are per recipient and apply when queued.
//! CK AddItem and CommonLib ExtraOriginalReference define concrete item moves.
//! xEdit REFR supplies loose quantity, charge and ownership fields.

use super::{Alloc, Handle, Pending, World};
use crate::inventory::{Bundle, Extra, Field, Stack};
use soe_esm::{GlobalFormId, Record};
use soe_papyrus::Value;

impl World {
    pub(super) fn loose_item(
        &self,
        base: Option<GlobalFormId>,
        owner: Option<GlobalFormId>,
        record: Option<&Record>,
    ) -> Option<Stack> {
        if !base.is_some_and(|base| self.is_inventory_base(base))
            || record.is_some_and(Record::is_deleted)
        {
            return None;
        }
        let mut extras = Vec::new();
        if let Some(owner) = owner {
            extras.push(Extra {
                kind: 33,
                fields: vec![Field::Form(Some(owner))],
            });
        }
        for (tag, kind) in [(b"XRNK", 35), (b"XCHG", 40)] {
            if let Some(bits) = record
                .and_then(|r| r.find(super::T(tag)))
                .and_then(super::le_u32)
            {
                let field = if kind == 40 {
                    Field::FloatBits(bits)
                } else {
                    Field::Word(bits)
                };
                extras.push(Extra {
                    kind,
                    fields: vec![field],
                });
            }
        }
        let count = record
            .and_then(|r| r.find(super::T(b"XCNT")))
            .and_then(super::le_u32)
            .map_or(1, |v| v as i32);
        Stack::new(count, extras).ok()
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
        self.inventory_reference_event(holder, item, delta, None, other, alloc);
    }

    fn inventory_reference_event(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        delta: i32,
        reference: Option<GlobalFormId>,
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
        let reference_value = self.form_value(reference, alloc);
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
                        reference_value.clone(),
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
        if count <= 0 {
            return false;
        }
        if !self.is_inventory_base(item) {
            return count == 1 && self.add_reference_notified(holder, item, alloc);
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

    fn container_changed(
        &mut self,
        reference: GlobalFormId,
        target: GlobalFormId,
        source: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) {
        let object = self.object_of(Handle::Form(reference), alloc);
        let new = self.form_value(Some(target), alloc);
        let old = self.form_value(source, alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnContainerChanged".into(),
            args: vec![new, old],
        });
    }

    fn add_reference_notified(
        &mut self,
        holder: GlobalFormId,
        reference: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> bool {
        if holder == reference {
            return false;
        }
        let mut matches = self.refs.iter().filter_map(|(&source, state)| {
            state
                .inventory
                .find_reference(reference)
                .map(|(base, stack)| (source, base, stack.count()))
        });
        let existing = matches.next();
        if matches.next().is_some() {
            return false;
        }
        let (source, base, count) = if let Some((source, base, count)) = existing {
            (Some(source), base, count)
        } else {
            let Some(state) = self.reference(reference) else {
                return false;
            };
            if state.deleted || state.destroyed {
                return false;
            }
            let (Some(base), Some(stack)) = (state.base, state.item.as_ref()) else {
                return false;
            };
            (None, base, stack.count())
        };
        if source == Some(holder) || count <= 0 || !self.is_inventory_base(base) {
            return false;
        }
        let Some(target) = self.reference(holder) else {
            return false;
        };
        if target.deleted
            || target.destroyed
            || target.inventory.count(base).checked_add(count).is_none()
        {
            return false;
        }
        if source.is_some() {
            if !self.refs.contains_key(&reference) {
                let state = self
                    .load_ref_record(reference)
                    .unwrap_or_else(|| super::RefState::new(Some(base)));
                if state.base != Some(base) || state.is_actor {
                    return false;
                }
                self.refs.insert(reference, state);
            }
            let Some(state) = self.refs.get_mut(&reference) else {
                return false;
            };
            if state.base != Some(base) || state.is_actor {
                return false;
            }
            state.item = None;
            state.enabled = false;
            state.cell = None;
            if reference.file == super::DYNAMIC_FILE {
                self.next_dynamic = self.next_dynamic.max(reference.object);
            }
        }
        let mut bundle = if let Some(source) = source {
            let Some((_, bundle)) = self
                .refs
                .get_mut(&source)
                .and_then(|state| state.inventory.take_reference(reference))
            else {
                return false;
            };
            bundle
        } else {
            let Some(state) = self.refs.get_mut(&reference) else {
                return false;
            };
            let Some(stack) = state.item.take() else {
                return false;
            };
            state.enabled = false;
            state.cell = None;
            Bundle::from_stack(stack.with_reference(reference))
        };
        bundle.unequip();
        // CONTEXT: Destination existence and capacity were checked before taking the source.
        if let Some(target) = self.refs.get_mut(&holder) {
            let result = target.inventory.receive(base, bundle);
            debug_assert!(result.is_ok());
        }
        if let Some(source) = source {
            self.inventory_reference_event(
                source,
                base,
                -count,
                Some(reference),
                Some(holder),
                alloc,
            );
        }
        self.inventory_reference_event(holder, base, count, Some(reference), source, alloc);
        self.container_changed(reference, holder, source, alloc);
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
        let bundle = self.remove_item_bundle(holder, item, count, target);
        let moved = bundle.count();
        if moved == 0 {
            return 0;
        }
        let mut plain = moved;
        for stack in bundle.stacks() {
            if let Some(reference) = stack.reference() {
                plain -= stack.count();
                self.inventory_reference_event(
                    holder,
                    item,
                    -stack.count(),
                    Some(reference),
                    target,
                    alloc,
                );
                if let Some(target) = target {
                    self.inventory_reference_event(
                        target,
                        item,
                        stack.count(),
                        Some(reference),
                        Some(holder),
                        alloc,
                    );
                    self.container_changed(reference, target, Some(holder), alloc);
                }
            }
        }
        self.inventory_event(holder, item, -plain, target, alloc);
        if let Some(target) = target {
            self.inventory_event(target, item, plain, Some(holder), alloc);
        }
        moved
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Inventory notification contracts from CK OnItemAdded/OnItemRemoved and
//! AddInventoryEventFilter. Filters are per recipient and apply when queued.
//! CK AddItem and CommonLib ExtraOriginalReference define concrete item moves.
//! xEdit REFR and SLGM supply loose quantity, charge, ownership and gem capacity.
//! CommonLib ExtraSoul supplies the per-instance filled soul level.

use super::{Alloc, Handle, Pending, World, T};
use crate::inventory::{Bundle, Error, Extra, Field, Selection, Stack, StackSource};
use soe_esm::{GlobalFormId, Record, Tag};
use soe_papyrus::{ObjectId, Value};

enum FormListAddition {
    Base(GlobalFormId, i32),
    Reference(GlobalFormId),
    Leveled(GlobalFormId, i32),
}

#[derive(Clone)]
struct ReferenceAddition {
    reference: GlobalFormId,
    source: Option<GlobalFormId>,
    base: GlobalFormId,
    count: i32,
    bundle: Bundle,
}

enum PreparedFormListAddition {
    Base(GlobalFormId, i32),
    Reference(ReferenceAddition),
    Leveled(Vec<(GlobalFormId, i32)>),
}

enum ReferenceSource {
    Loose,
    Held(GlobalFormId),
    Ambiguous,
}

impl World {
    /// Exchange an exact quantity of merchandise and currency between two
    /// holders after preparing all four inventory changes.
    ///
    /// # Errors
    /// Returns an inventory error when either holder is missing, either
    /// source lacks the requested quantity, or a recipient would overflow.
    pub fn trade_items_notified(
        &mut self,
        item_source: GlobalFormId,
        item: GlobalFormId,
        item_count: i32,
        item_target: GlobalFormId,
        currency: GlobalFormId,
        currency_count: i32,
        alloc: Alloc<'_>,
    ) -> Result<(), Error> {
        if item_count <= 0 || currency_count <= 0 || item_source == item_target {
            return Err(Error::Quantity);
        }
        let mut source_items = self
            .reference(item_source)
            .ok_or(Error::Holder)?
            .inventory
            .clone();
        let mut target_items = self
            .reference(item_target)
            .ok_or(Error::Holder)?
            .inventory
            .clone();
        if source_items.count(item) < item_count || target_items.count(currency) < currency_count {
            return Err(Error::Selection);
        }
        let item_bundle = source_items.take(item, item_count);
        let mut currency_bundle = target_items.take(currency, currency_count);
        currency_bundle.unequip();
        target_items.receive(item, item_bundle)?;
        source_items.receive(currency, currency_bundle)?;

        self.reference(item_source).ok_or(Error::Holder)?.inventory = source_items;
        self.reference(item_target).ok_or(Error::Holder)?.inventory = target_items;

        self.inventory_event(item_source, item, -item_count, Some(item_target), alloc);
        self.inventory_event(item_target, item, item_count, Some(item_source), alloc);
        self.inventory_event(
            item_target,
            currency,
            -currency_count,
            Some(item_source),
            alloc,
        );
        self.inventory_event(
            item_source,
            currency,
            currency_count,
            Some(item_target),
            alloc,
        );
        Ok(())
    }

    /// Add an inventory event filter to one Papyrus listener.
    /// Filters on the same listener stack and match either a form or members
    /// of a form list when inventory events are queued.
    pub fn add_inventory_event_filter(&mut self, listener: ObjectId, filter: GlobalFormId) {
        self.inventory_filters
            .entry(listener)
            .or_default()
            .push(filter);
    }

    /// Remove a form filter from one Papyrus listener.
    pub fn remove_inventory_event_filter(
        &mut self,
        listener: ObjectId,
        filter: Option<GlobalFormId>,
    ) {
        if let Some(filters) = self.inventory_filters.get_mut(&listener) {
            filters.retain(|candidate| Some(*candidate) != filter);
        }
    }

    /// Remove every inventory event filter from one Papyrus listener.
    pub fn remove_all_inventory_event_filters(&mut self, listener: ObjectId) {
        self.inventory_filters.remove(&listener);
    }

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
                156 => Some(*b"SLGM"),
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
        if self.brewed_potions.contains_key(&item) {
            return true;
        }
        self.record(item).is_some_and(|record| {
            !record.is_deleted()
                && matches!(
                    record.tag.as_bytes(),
                    b"ALCH"
                        | b"AMMO"
                        | b"APPA"
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

    /// Queue a count change for the reference, its running quest aliases and
    /// active effects targeting that reference.
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
        let effects: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| effect.target == holder)
            .map(|effect| effect.id)
            .collect();
        for id in effects {
            recipients.push(self.object_of(Handle::ActiveEffect(id), alloc));
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

    /// Add one LVLI result at the caller's resolved selection level.
    /// Validation and capacity failures leave inventory, events and RNG unchanged.
    pub fn add_leveled_item_notified(
        &mut self,
        holder: GlobalFormId,
        list: GlobalFormId,
        level: u32,
        count: i32,
        alloc: Alloc<'_>,
    ) -> bool {
        if count <= 0 {
            return false;
        }
        let Some(reference) = self.reference(holder) else {
            return false;
        };
        let mut updated = reference.inventory.clone();
        let previous_rng = self.rng_streams;
        let Ok(mut selected) =
            self.roll_leveled_inventory_context(list, level, count, Some(self.player_level))
        else {
            return false;
        };
        let changes: Vec<_> = selected
            .iter()
            .map(|(item, count)| (*item, count))
            .collect();
        for &(item, count) in &changes {
            if updated.receive(item, selected.take(item, count)).is_err() {
                self.rng_streams = previous_rng;
                return false;
            }
        }
        let Some(reference) = self.reference(holder) else {
            self.rng_streams = previous_rng;
            return false;
        };
        reference.inventory = updated;
        for (item, count) in changes {
            self.inventory_event(holder, item, count, None, alloc);
        }
        true
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
        if self.tag_of(item) == Some(T(b"LVLI")) {
            let Ok(level) = self.encounter_level(holder) else {
                return false;
            };
            return self.add_leveled_item_notified(holder, item, level, count, alloc);
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

    /// Add harvested produce to an actor and dispatch the CommonLib harvest event.
    /// Failed or non-actor harvest attempts leave inventory and event sources unchanged.
    pub fn harvest_item(
        &mut self,
        harvester: GlobalFormId,
        produce_item: GlobalFormId,
        count: i32,
        alloc: Alloc<'_>,
    ) -> bool {
        if count <= 0
            || !self.is_inventory_base(produce_item)
            || self.actor_ref(harvester).is_none()
            || !self.add_item_notified(harvester, produce_item, count, alloc)
        {
            return false;
        }
        self.commonlib_harvested(produce_item, harvester);
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
        let Some(addition) = self.reference_addition(holder, reference) else {
            return false;
        };
        let Some(target) = self.reference(holder) else {
            return false;
        };
        if target.deleted
            || target.destroyed
            || target
                .inventory
                .count(addition.base)
                .checked_add(addition.count)
                .is_none()
        {
            return false;
        }
        let result = target
            .inventory
            .receive(addition.base, addition.bundle.clone());
        debug_assert!(result.is_ok());
        self.apply_reference_addition(holder, &addition, alloc);
        true
    }

    fn reference_source(&self, reference: GlobalFormId) -> ReferenceSource {
        let mut matches = self.refs.iter().filter_map(|(&source, state)| {
            state.inventory.find_reference(reference).map(|_| source)
        });
        let existing = matches.next();
        if matches.next().is_some() {
            return ReferenceSource::Ambiguous;
        }
        existing.map_or(ReferenceSource::Loose, ReferenceSource::Held)
    }

    fn reference_addition(
        &mut self,
        holder: GlobalFormId,
        reference: GlobalFormId,
    ) -> Option<ReferenceAddition> {
        if holder == reference {
            return None;
        }
        let source = match self.reference_source(reference) {
            ReferenceSource::Loose => None,
            ReferenceSource::Held(source) => Some(source),
            ReferenceSource::Ambiguous => return None,
        };
        let (base, count, bundle) = if let Some(source) = source {
            let (base, stack) = self
                .refs
                .get(&source)?
                .inventory
                .find_reference(reference)?;
            let count = stack.count();
            let bundle = Bundle::from_stack(stack.clone());
            let valid = if let Some(state) = self.refs.get(&reference) {
                state.base == Some(base) && !state.is_actor
            } else {
                let state = self
                    .load_ref_record(reference)
                    .unwrap_or_else(|| super::RefState::new(Some(base)));
                let valid = state.base == Some(base) && !state.is_actor;
                if valid {
                    self.refs.insert(reference, state);
                }
                valid
            };
            if !valid {
                return None;
            }
            (base, count, bundle)
        } else {
            let state = self.reference(reference)?;
            if state.deleted || state.destroyed {
                return None;
            }
            let (Some(base), Some(stack)) = (state.base, state.item.as_ref()) else {
                return None;
            };
            (
                base,
                stack.count(),
                Bundle::from_stack(stack.clone().with_reference(reference)),
            )
        };
        if source == Some(holder) || count <= 0 || !self.is_inventory_base(base) {
            return None;
        }
        let mut bundle = bundle;
        bundle.unequip();
        Some(ReferenceAddition {
            reference,
            source,
            base,
            count,
            bundle,
        })
    }

    fn apply_reference_addition(
        &mut self,
        holder: GlobalFormId,
        addition: &ReferenceAddition,
        alloc: Alloc<'_>,
    ) {
        if let Some(source) = addition.source {
            let Some(state) = self.refs.get_mut(&addition.reference) else {
                unreachable!("preflight loaded held reference");
            };
            debug_assert_eq!(state.base, Some(addition.base));
            debug_assert!(!state.is_actor);
            state.item = None;
            state.enabled = false;
            state.cell = None;
            if addition.reference.file == super::DYNAMIC_FILE {
                self.next_dynamic = self.next_dynamic.max(addition.reference.object);
            }
            let removed = self
                .refs
                .get_mut(&source)
                .and_then(|state| state.inventory.take_reference(addition.reference));
            debug_assert!(removed.is_some());
            self.inventory_reference_event(
                source,
                addition.base,
                -addition.count,
                Some(addition.reference),
                Some(holder),
                alloc,
            );
        } else {
            let Some(state) = self.refs.get_mut(&addition.reference) else {
                unreachable!("preflight loaded reference");
            };
            let removed = state.item.take();
            debug_assert!(removed.is_some());
            state.enabled = false;
            state.cell = None;
        }
        self.inventory_reference_event(
            holder,
            addition.base,
            addition.count,
            Some(addition.reference),
            addition.source,
            alloc,
        );
        self.container_changed(addition.reference, holder, addition.source, alloc);
    }

    /// Add every supported item reachable through a FormList in one transaction.
    /// Lists are flattened in member order. A depth cap and ancestry check reject
    /// malformed cycles before inventory mutation or event queueing. Leveled
    /// leaves use the receiver encounter level and the AddItem player context.
    pub fn add_form_list_notified(
        &mut self,
        holder: GlobalFormId,
        list: GlobalFormId,
        count: i32,
        alloc: Alloc<'_>,
    ) -> bool {
        if count <= 0 || self.reference(holder).is_none() {
            return false;
        }
        let Ok(level) = self.encounter_level(holder) else {
            return false;
        };
        let Some(additions) = self.form_list_additions(list, count, &mut Vec::new()) else {
            return false;
        };
        let target_inventory = {
            let Some(target) = self.reference(holder) else {
                return false;
            };
            if target.deleted || target.destroyed {
                return false;
            }
            target.inventory.clone()
        };
        let previous_rng = self.rng_streams;
        let mut updated = target_inventory;
        let mut prepared = Vec::new();
        let mut references = Vec::new();
        for addition in additions {
            match addition {
                FormListAddition::Base(item, total) => {
                    if updated.add(item, total).is_err() {
                        self.rng_streams = previous_rng;
                        return false;
                    }
                    prepared.push(PreparedFormListAddition::Base(item, total));
                }
                FormListAddition::Reference(reference) => {
                    if references.contains(&reference) {
                        self.rng_streams = previous_rng;
                        return false;
                    }
                    references.push(reference);
                    let Some(addition) = self.reference_addition(holder, reference) else {
                        self.rng_streams = previous_rng;
                        return false;
                    };
                    if updated
                        .receive(addition.base, addition.bundle.clone())
                        .is_err()
                    {
                        self.rng_streams = previous_rng;
                        return false;
                    }
                    prepared.push(PreparedFormListAddition::Reference(addition));
                }
                FormListAddition::Leveled(list, count) => {
                    let Ok(mut selected) = self.roll_leveled_inventory_context(
                        list,
                        level,
                        count,
                        Some(self.player_level),
                    ) else {
                        self.rng_streams = previous_rng;
                        return false;
                    };
                    let changes: Vec<_> = selected
                        .iter()
                        .map(|(item, total)| (*item, total))
                        .collect();
                    for (item, total) in &changes {
                        if updated
                            .receive(*item, selected.take(*item, *total))
                            .is_err()
                        {
                            self.rng_streams = previous_rng;
                            return false;
                        }
                    }
                    prepared.push(PreparedFormListAddition::Leveled(changes));
                }
            }
        }
        let Some(target) = self.reference(holder) else {
            self.rng_streams = previous_rng;
            return false;
        };
        target.inventory = updated;
        for addition in prepared {
            match addition {
                PreparedFormListAddition::Base(item, total) => {
                    self.inventory_event(holder, item, total, None, alloc);
                }
                PreparedFormListAddition::Reference(addition) => {
                    self.apply_reference_addition(holder, &addition, alloc);
                }
                PreparedFormListAddition::Leveled(changes) => {
                    for (item, total) in changes {
                        self.inventory_event(holder, item, total, None, alloc);
                    }
                }
            }
        }
        true
    }

    /// Flatten a list before applying it. The Creation Kit says AddItem adds
    /// each FormList item, but does not specify nested-list behavior. The
    /// recursive policy matches this project's FormList inventory-count walk.
    fn form_list_additions(
        &mut self,
        list: GlobalFormId,
        count: i32,
        visiting: &mut Vec<GlobalFormId>,
    ) -> Option<Vec<FormListAddition>> {
        if visiting.len() >= 64 || visiting.contains(&list) {
            return None;
        }
        let items = self.form_list(list)?.clone();
        visiting.push(list);
        let result = (|| {
            let mut additions = Vec::new();
            for item in items {
                let entries = if self.tag_of(item) == Some(Tag::new(b"FLST")) {
                    self.form_list_additions(item, count, visiting)?
                } else if self.is_inventory_base(item) {
                    vec![FormListAddition::Base(item, count)]
                } else if self.tag_of(item) == Some(Tag::new(b"LVLI")) {
                    vec![FormListAddition::Leveled(item, count)]
                } else if self.tag_of(item) == Some(Tag::new(b"REFR")) && count == 1 {
                    vec![FormListAddition::Reference(item)]
                } else {
                    return None;
                };
                for entry in entries {
                    match entry {
                        FormListAddition::Base(item, count) => {
                            if let Some(FormListAddition::Base(_, total)) = additions
                                .iter_mut()
                                .find(|entry| matches!(entry, FormListAddition::Base(form, _) if *form == item))
                            {
                                *total = total.checked_add(count)?;
                            } else {
                                additions.push(FormListAddition::Base(item, count));
                            }
                        }
                        FormListAddition::Reference(item) => additions.push(FormListAddition::Reference(item)),
                        FormListAddition::Leveled(item, count) => {
                            additions.push(FormListAddition::Leveled(item, count));
                        }
                    }
                }
            }
            Some(additions)
        })();
        visiting.pop();
        result
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

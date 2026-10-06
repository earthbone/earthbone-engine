// SPDX-License-Identifier: GPL-3.0-or-later

//! Native QuickLoot IE behavior adapted from LootMenu.cpp, LootMenuManager.cpp,
//! LootMenu.Sorting.cpp and Items/ItemStack.cpp by Miss Corruption, AtomCrafty,
//! Faen, Ryan McKenzie and contributors. GPL-3.0-or-later. The original
//! modding/linking exceptions are recorded in docs/godot-host.md.
//! Inventories remain in Game. This value owns only transient presentation.

use soe_commonlib_shim::quick_loot::{
    ButtonDefinition, Event, EventKind, Handle, HandleResult, ItemStack, ItemValue,
    QuickLootAction, QuickLootApi,
};
use soe_esm::{GlobalFormId, Tag};
use soe_game::inventory::{Field, StackSource};
use soe_game::loot::LootState;
use soe_game::Game;

use crate::inventory_menu::{self, Category};
use crate::item_card::ItemCard;

/// Sort choices share QuickLoot's ascending name/weight and descending value rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort { #[default] Name, Value, Weight, Type }

impl Sort {
    /// Advance the visible sort choice.
    pub fn next(self) -> Self {
        match self { Self::Name => Self::Value, Self::Value => Self::Weight,
            Self::Weight => Self::Type, Self::Type => Self::Name }
    }
    /// Label for the sort control.
    pub fn label(self) -> &'static str {
        match self { Self::Name => "Name", Self::Value => "Value", Self::Weight => "Weight", Self::Type => "Type" }
    }
}

/// One derived list row. Stack indices are revalidated before every transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub stack: ItemStack,
    pub name: String,
    pub category: Category,
    pub stealing: bool,
    pub value: u32,
    pub weight: f32,
}

/// Crosshair menu state and the mod API instance used by the native host.
#[derive(Default)]
pub struct QuickLoot {
    pub api: QuickLootApi,
    pub holder: Option<GlobalFormId>,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub sort: Sort,
    pub locked: bool,
    pub info: Vec<String>,
    pub buttons: Vec<ButtonDefinition>,
    last_selection: Option<(GlobalFormId, usize)>,
}

/// Resolved FormIDs remain independent of Skyrim's load-order byte encoding.
pub fn handle(id: GlobalFormId) -> Handle { Handle((u64::from(id.file) << 32) | u64::from(id.object)) }

/// Convert a source-level API identity back to a resolved FormID.
pub fn form(handle: Handle) -> Option<GlobalFormId> {
    Some(GlobalFormId { file: u16::try_from(handle.0 >> 32).ok()?, object: handle.0 as u32 })
}

impl QuickLoot {
    fn event(&self, kind: EventKind, game: &Game) -> Option<Event> {
        let mut event = Event::new(kind, handle(self.holder?));
        event.actor = Some(handle(game.player()));
        event.stack = self.rows.get(self.selected).map(|row| row.stack.clone());
        Some(event)
    }

    /// Close without pausing Game or taking ownership of the mouse.
    pub fn close(&mut self) {
        if let Some(holder) = self.holder.take() {
            self.last_selection = Some((holder, self.selected));
            let mut event = Event::new(EventKind::CloseLootMenu, handle(holder));
            self.api.dispatch(&mut event);
        }
        self.rows.clear();
        self.info.clear();
        self.buttons.clear();
        self.locked = false;
    }

    /// Observe the shared crosshair reference, hiding during modal UI, grabs or
    /// beast form. Looking does not activate a container or run its scripts.
    pub fn refresh(&mut self, game: &Game, focused: Option<GlobalFormId>, blocked: bool) {
        let target = self.api.forced_container().and_then(form).or(focused);
        let mut world = game.world_mut();
        let state = target.map_or(LootState::Hidden, |id| world.loot_state(id));
        if blocked || !self.api.enabled() || world.grabbed_reference().is_some()
            || world.beast_form || state == LootState::Hidden {
            drop(world);
            self.close();
            return;
        }
        drop(world);
        if self.holder != target {
            self.close();
            let Some(holder) = target else { return };
            let mut opening = Event::new(EventKind::OpeningLootMenu, handle(holder));
            self.api.dispatch(&mut opening);
            if opening.result == HandleResult::Stop { return; }
            self.holder = Some(holder);
            self.selected = self.last_selection.filter(|(previous, _)| *previous == holder).map_or(0, |(_, index)| index);
            self.api.dispatch(&mut Event::new(EventKind::OpenLootMenu, handle(holder)));
        }
        self.locked = state == LootState::Locked;
        let selected = self.rows.get(self.selected).map(|row| row.stack.clone());
        let rows = if self.locked { Vec::new() } else { self.read_rows(game) };
        let changed = self.rows != rows;
        self.rows = rows;
        if let Some(index) = selected.and_then(|selected| self.rows.iter().position(|row| row.stack.item == selected.item && row.stack.extra_index == selected.extra_index)) {
            self.selected = index;
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        if changed { self.notify_selection(game); }
        self.refresh_bars(game);
    }

    fn read_rows(&mut self, game: &Game) -> Vec<Row> {
        let Some(holder) = self.holder else { return Vec::new() };
        let entries = inventory_menu::entries(game, holder, Category::All);
        let mut rows = Vec::new();
        for entry in entries {
            let mut world = game.world_mut();
            let Some(record) = world.record(entry.item) else { continue };
            if !can_display(record) || entry.name == "Unnamed item" { continue; }
            let Some(reference) = world.reference_ref(holder) else { continue };
            let inventory = &reference.inventory;
            let represented: i64 = inventory.stacks(entry.item).iter().map(|stack| i64::from(stack.count())).sum();
            if represented > i64::from(entry.count) { continue; }
            let mut sources = Vec::new();
            if represented < i64::from(entry.count) { sources.push((StackSource::Plain, entry.count - represented as i32)); }
            sources.extend(inventory.stacks(entry.item).iter().enumerate().filter(|(_, stack)| stack.count() > 0).map(|(index, stack)| (StackSource::Extra(index), stack.count())));
            let card = ItemCard::read(&mut world, entry.item).ok();
            for (source, count) in sources {
                let Some(reference) = world.reference_ref(holder) else { continue };
                let Ok(selection) = reference.inventory.select(entry.item, source, count) else { continue };
                let stack = reference.inventory.selected_stack(&selection).ok().flatten();
                let name = stack.and_then(|stack| stack.extra(153)).and_then(|extra| extra.fields.iter().find_map(|field| {
                    if let Field::Text(text) = field { String::from_utf8(text.clone()).ok() } else { None }
                })).unwrap_or_else(|| entry.name.clone());
                let stealing = world.loot_is_stealing(holder, game.player(), &selection).unwrap_or(true);
                rows.push(Row { stack: ItemStack { item: handle(entry.item), count, extra_index: match source {
                    StackSource::Plain => None, StackSource::Extra(index) => Some(index)
                }, drop_ref: None }, name, category: entry.category, stealing,
                    value: card.as_ref().map_or(0, |card| card.value), weight: card.as_ref().map_or(0.0, |card| card.weight) });
            }
        }
        let mut event = Event::new(EventKind::ModifyInventory, handle(holder));
        event.inventory = rows.iter().map(|row| row.stack.clone()).collect();
        self.api.dispatch(&mut event);
        let mut modified = Vec::new();
        for stack in event.inventory {
            if modified.iter().any(|row: &Row| row.stack == stack) { continue; }
            if let Some(row) = rows.iter().find(|row| row.stack == stack) { modified.push(row.clone()); }
        }
        let mut event = Event::new(EventKind::InvalidateLootMenu, handle(holder));
        event.inventory = modified.iter().map(|row| row.stack.clone()).collect();
        self.api.dispatch(&mut event);
        for row in &mut modified {
            let mut event = Event::new(EventKind::ModifyItemData, handle(holder));
            event.stack = Some(row.stack.clone());
            event.data.insert("displayName".into(), ItemValue::Text(row.name.clone()));
            event.data.insert("stolen".into(), ItemValue::Bool(row.stealing));
            self.api.dispatch(&mut event);
            if let Some(ItemValue::Text(name)) = event.data.get("displayName") { row.name.clone_from(name); }
        }
        modified.sort_by(|a, b| {
            let primary = match self.sort {
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Value => b.value.cmp(&a.value),
                Sort::Weight => a.weight.total_cmp(&b.weight),
                Sort::Type => (a.category as u8).cmp(&(b.category as u8)),
            };
            primary.then_with(|| a.name.cmp(&b.name)).then_with(|| a.stack.item.cmp(&b.stack.item))
                .then_with(|| a.stack.extra_index.cmp(&b.stack.extra_index))
        });
        modified
    }

    fn notify_selection(&mut self, game: &Game) {
        if let Some(mut event) = self.event(EventKind::SelectItem, game).filter(|event| event.stack.is_some()) { self.api.dispatch(&mut event); }
    }

    fn refresh_bars(&mut self, game: &Game) {
        let Some(mut info) = self.event(EventKind::PopulateInfoBar, game) else { return };
        if let Some(row) = self.rows.get(self.selected) { info.info.push(format!("Weight {}   Value {}", row.weight, row.value)); }
        self.api.dispatch(&mut info);
        self.info = info.info;
        let Some(mut buttons) = self.event(EventKind::PopulateButtonBar, game) else { return };
        self.api.dispatch(&mut buttons);
        let stealing = self.rows.get(self.selected).is_some_and(|row| row.stealing);
        let mut defaults = vec![
            ButtonDefinition { label: if stealing { "Steal" } else { "Take" }.into(), button_art_index: 0, stealing, action: QuickLootAction::Take },
            ButtonDefinition { label: if self.rows.iter().any(|row| row.stealing) { "Steal all" } else { "Take all" }.into(), button_art_index: 0, stealing, action: QuickLootAction::TakeAll },
            ButtonDefinition { label: if self.locked { "Unlock" } else { "Search" }.into(), button_art_index: 0, stealing: false, action: QuickLootAction::Transfer },
        ];
        defaults.extend(buttons.buttons);
        buttons.kind = EventKind::ModifyButtonBar;
        buttons.buttons = defaults;
        self.api.dispatch(&mut buttons);
        self.buttons = buttons.buttons;
    }

    /// Select with clamped scroll and page actions. No wrapping at either end.
    pub fn scroll(&mut self, game: &Game, delta: i32) {
        self.selected = self.selected.saturating_add_signed(delta as isize).min(self.rows.len().saturating_sub(1));
        self.notify_selection(game);
        self.refresh_bars(game);
    }

    /// Dispatch cancellable 2.1 actions before the host applies them.
    pub fn accepts_action(&mut self, game: &Game, action: QuickLootAction) -> bool {
        let Some(mut event) = self.event(EventKind::InputAction, game) else { return false };
        event.action = action;
        self.api.dispatch(&mut event);
        event.result == HandleResult::Continue
    }

    fn take_row(&mut self, game: &mut Game, row: &Row, count: i32) -> i32 {
        let Some(holder) = self.holder else { return 0 };
        let Some(item) = form(row.stack.item) else { return 0 };
        let source = row.stack.extra_index.map_or(StackSource::Plain, StackSource::Extra);
        let selected = game.world().reference_ref(holder).and_then(|reference| reference.inventory.select(item, source, count).ok());
        let Some(selected) = selected else { return 0 };
        let Some(mut event) = self.event(EventKind::TakingItem, game) else { return 0 };
        event.stack = Some(row.stack.clone());
        self.api.dispatch(&mut event);
        if event.result == HandleResult::Stop { return 0; }
        let moved = game.loot_item(holder, &selected).unwrap_or(0);
        if moved > 0 {
            event.kind = EventKind::TakeItem;
            if let Some(stack) = event.stack.as_mut() { stack.count = moved; }
            self.api.dispatch(&mut event);
        }
        moved
    }

    /// Take one selected item, or every displayed stack. Rejected rows remain.
    pub fn take(&mut self, game: &mut Game, all: bool) -> i32 {
        if self.locked { return 0; }
        let mut rows = if all { self.rows.clone() } else { self.rows.get(self.selected).cloned().into_iter().collect() };
        rows.sort_by(|a, b| a.stack.item.cmp(&b.stack.item).then_with(|| b.stack.extra_index.cmp(&a.stack.extra_index)));
        let mut moved = 0_i32;
        for row in &rows { moved = moved.saturating_add(self.take_row(game, row, if all { row.stack.count } else { 1 })); }
        let holder = self.holder;
        self.refresh(game, holder, false);
        moved
    }
}

fn can_display(record: &soe_esm::Record) -> bool {
    if record.is_deleted() { return false; }
    match record.tag.as_bytes() {
        b"WEAP" => record.find(Tag(*b"DNAM")).is_none_or(|bytes| bytes.get(12).is_none_or(|flags| flags & 0x80 == 0)),
        b"ARMO" => record.flags & 4 == 0,
        b"AMMO" => record.find(Tag(*b"DATA")).is_none_or(|bytes| bytes.get(4).is_none_or(|flags| flags & 2 == 0)),
        b"LIGH" => record.find(Tag(*b"DATA")).and_then(|bytes| bytes.get(12)).is_some_and(|flags| flags & 2 != 0),
        b"SCRL" | b"BOOK" | b"INGR" | b"MISC" | b"KEYM" | b"ALCH" | b"SLGM" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;

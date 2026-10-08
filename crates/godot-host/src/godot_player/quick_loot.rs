// SPDX-License-Identifier: GPL-3.0-or-later

//! Crosshair QuickLoot presenter and input over the shared Game inventory.

use godot::classes::control::LayoutPreset;
use godot::classes::{
    Button, CanvasLayer, Control, HBoxContainer, Label, PanelContainer, ScrollContainer,
    VBoxContainer,
};
use godot::global::Key;
use godot::prelude::*;
use soe_commonlib_shim::quick_loot::QuickLootAction;
use soe_esm::GlobalFormId;
use soe_game::loot::LootState;

use super::PlayerNode;
use crate::quick_loot::QuickLootView;

impl PlayerNode {
    pub(super) fn hide_quick_loot(&mut self) {
        self.quick_loot.close();
        if let Some(mut view) = self.quick_loot_view.take() {
            if view.layer.is_instance_valid() {
                view.layer.queue_free();
            }
        }
    }

    pub(super) fn update_quick_loot(&mut self) {
        let Some(game) = self.game.as_ref() else {
            self.hide_quick_loot();
            return;
        };
        let focused = game.world().crosshair_reference();
        let blocked = self.menu.is_open() || !self.input_focused;
        self.quick_loot.refresh(game, focused, blocked);
        if self.quick_loot.holder.is_none() {
            if self.quick_loot_view.is_some() {
                self.hide_quick_loot();
            }
            return;
        }
        if self.quick_loot_view.is_none() {
            self.quick_loot_view = Some(QuickLootView::create(self));
        }
        if let (Some(view), Some(game)) = (self.quick_loot_view.as_mut(), self.game.as_ref()) {
            view.update(&self.quick_loot, game);
        }
        let preview = self
            .quick_loot_view
            .as_ref()
            .and_then(|view| view.selected)
            .filter(|item| {
                self.quick_loot_view
                    .as_ref()
                    .is_some_and(|view| view.card.selected != Some(*item))
            });
        if let Some(item) = preview {
            self.preview_inventory_item(i64::from(item.file), i64::from(item.object));
        }
    }

    pub(super) fn quick_loot_input(&mut self, event: &Gd<godot::classes::InputEvent>) -> bool {
        let Ok(key) = event.clone().try_cast::<godot::classes::InputEventKey>() else {
            return false;
        };
        if !key.is_pressed() || key.is_echo() || self.quick_loot.holder.is_none() {
            return false;
        }
        match key.get_keycode() {
            Key::UP => {
                if let Some(game) = self.game.as_ref() {
                    self.quick_loot.scroll(game, -1);
                }
            }
            Key::DOWN => {
                if let Some(game) = self.game.as_ref() {
                    self.quick_loot.scroll(game, 1);
                }
            }
            Key::E => {
                if self
                    .game
                    .as_ref()
                    .is_some_and(|game| self.quick_loot.accepts_action(game, QuickLootAction::Take))
                {
                    if let Some(game) = self.game.as_mut() {
                        self.quick_loot.take(game, false);
                    }
                }
            }
            Key::T => {
                if self.game.as_ref().is_some_and(|game| {
                    self.quick_loot
                        .accepts_action(game, QuickLootAction::TakeAll)
                }) {
                    if let Some(game) = self.game.as_mut() {
                        self.quick_loot.take(game, true);
                    }
                }
            }
            Key::R => {
                self.quick_loot.sort = self.quick_loot.sort.next();
                self.update_quick_loot();
            }
            Key::TAB | Key::ENTER => {
                if let Some(holder) = self.quick_loot.holder {
                    self.open_container_menu(holder);
                }
            }
            Key::ESCAPE => self.hide_quick_loot(),
            _ => return false,
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Take the selected stack or all visible stacks through Game's transaction.
    #[func]
    pub fn quick_loot_take(&mut self, all: bool) -> i64 {
        let action = if all {
            QuickLootAction::TakeAll
        } else {
            QuickLootAction::Take
        };
        if !self
            .game
            .as_ref()
            .is_some_and(|game| self.quick_loot.accepts_action(game, action))
        {
            return 0;
        }
        let Some(game) = self.game.as_mut() else {
            return 0;
        };
        i64::from(self.quick_loot.take(game, all))
    }

    /// Move the active list selection to an existing row.
    #[func]
    pub fn quick_loot_select(&mut self, index: i64) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if index >= self.quick_loot.rows.len() {
            return false;
        }
        let delta = index as isize - self.quick_loot.selected as isize;
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        self.quick_loot.scroll(game, delta as i32);
        self.update_quick_loot();
        true
    }

    /// Force a container into the crosshair presentation, matching QuickLoot's mod API.
    #[func]
    pub fn quick_loot_force_container(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let holder = GlobalFormId { file, object };
        if game.world_mut().loot_state(holder) == LootState::Hidden {
            return false;
        }
        self.quick_loot
            .api
            .init(
                "godot-script",
                soe_commonlib_shim::quick_loot::ApiVersion::V21,
            )
            .ok();
        self.quick_loot
            .api
            .force_current_container("godot-script", crate::quick_loot::handle(holder))
            .is_ok()
    }

    /// Clear a forced target and return to the crosshair ray.
    #[func]
    pub fn quick_loot_clear_forced_container(&mut self) {
        let _ = self.quick_loot.api.clear_forced_container("godot-script");
    }

    /// Read the visible target and row count for a host scene test or overlay.
    #[func]
    pub fn quick_loot_snapshot(&self) -> VarDictionary {
        let mut snapshot = VarDictionary::new();
        snapshot.set("open", self.quick_loot.holder.is_some());
        snapshot.set("count", self.quick_loot.rows.len() as i64);
        snapshot.set(
            "total",
            self.quick_loot
                .rows
                .iter()
                .map(|row| row.stack.count)
                .sum::<i32>() as i64,
        );
        snapshot.set("locked", self.quick_loot.locked);
        snapshot.set("selected", self.quick_loot.selected as i64);
        snapshot.set("sort", self.quick_loot.sort.label());
        snapshot.set(
            "stealing",
            self.quick_loot
                .rows
                .iter()
                .filter(|row| row.stealing)
                .count() as i64,
        );
        let mut names = Array::<GString>::new();
        for row in &self.quick_loot.rows {
            names.push(&GString::from(row.name.as_str()));
        }
        snapshot.set("names", &names);
        snapshot
    }
}

impl QuickLootView {
    pub(super) fn create(player: &mut PlayerNode) -> Self {
        let mut layer = CanvasLayer::new_alloc();
        layer.set_name("QuickLootLayer");
        layer.set_layer(80);
        let mut root = Control::new_alloc();
        root.set_name("QuickLootRoot");
        root.set_anchors_preset(LayoutPreset::FULL_RECT);
        layer.add_child(&root);
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("QuickLootPanel");
        panel.set_position(Vector2::new(330.0, 115.0));
        panel.set_custom_minimum_size(Vector2::new(245.0, 0.0));
        let mut column = VBoxContainer::new_alloc();
        let mut heading = Label::new_alloc();
        heading.set_name("QuickLootTitle");
        heading.set_text("Loot");
        column.add_child(&heading);
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_name("QuickLootScroll");
        scroll.set_custom_minimum_size(Vector2::new(225.0, 170.0));
        let mut rows = VBoxContainer::new_alloc();
        rows.set_name("QuickLootRows");
        scroll.add_child(&rows);
        column.add_child(&scroll);
        let mut state = Label::new_alloc();
        state.set_name("QuickLootState");
        column.add_child(&state);
        let mut controls = Label::new_alloc();
        controls.set_name("QuickLootControls");
        controls.set_text("E Take   T Take All   Tab Open   R Sort");
        column.add_child(&controls);
        let mut card_host = HBoxContainer::new_alloc();
        card_host.set_name("QuickLootItemCardHost");
        let card =
            crate::godot_player::item_card::ItemCardView::create_sized(&mut card_host, 220.0);
        column.add_child(&card_host);
        panel.add_child(&column);
        root.add_child(&panel);
        player.base_mut().add_child(&layer);
        Self {
            layer,
            title: heading,
            rows,
            state,
            selected: None,
            signature: Vec::new(),
            card,
        }
    }

    pub(super) fn update(&mut self, loot: &crate::quick_loot::QuickLoot, _game: &soe_game::Game) {
        let signature: Vec<String> = loot
            .rows
            .iter()
            .map(|row| format!("{}:{}:{}", row.name, row.stack.count, row.stealing))
            .collect();
        if signature != self.signature {
            self.signature = signature;
            for mut child in self.rows.get_children().iter_shared() {
                self.rows.remove_child(&child);
                child.queue_free();
            }
            if loot.rows.is_empty() {
                let mut empty = Label::new_alloc();
                empty.set_text(if loot.locked { "Locked" } else { "Empty" });
                self.rows.add_child(&empty);
            } else {
                for (index, row) in loot.rows.iter().enumerate() {
                    let mut button = Button::new_alloc();
                    let icon = match row.category {
                        crate::inventory_menu::Category::Weapons => "⚔",
                        crate::inventory_menu::Category::Apparel => "◇",
                        crate::inventory_menu::Category::Food
                        | crate::inventory_menu::Category::Potions => "✚",
                        _ => "•",
                    };
                    let marker = if row.stealing { "  [Steal]" } else { "" };
                    let selected = if index == loot.selected { "> " } else { "  " };
                    button.set_text(&format!(
                        "{selected}{icon} {} x{}{}",
                        row.name, row.stack.count, marker
                    ));
                    button.set_name(&format!("QuickLootRow{index}"));
                    self.rows.add_child(&button);
                }
            }
        }
        self.selected = loot
            .rows
            .get(loot.selected)
            .and_then(|row| crate::quick_loot::form(row.stack.item));
        self.title.set_text("Loot");
        self.state.set_text(if loot.locked {
            "Locked container"
        } else if loot.rows.is_empty() {
            "Empty"
        } else {
            loot.sort.label()
        });
    }

    pub(super) fn tick(&mut self, delta: f64) {
        self.card.tick(delta);
    }
}

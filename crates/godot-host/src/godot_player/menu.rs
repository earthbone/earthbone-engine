// SPDX-License-Identifier: GPL-3.0-or-later

//! Native modal UI from Godot Control/Theme and gdext node ownership examples.
//! PlayerNode keeps ticking the shared Game while this fixed control tree is open.

use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::input::MouseMode;
use godot::classes::{
    Button, CanvasLayer, Control, Input, Label, PanelContainer, ScrollContainer, StyleBoxFlat,
    Theme, VBoxContainer,
};
use godot::prelude::*;

use super::PlayerNode;
use crate::crafting_menu;
use crate::dialogue_menu;
use crate::inventory_menu::{self, Category};
use soe_esm::GlobalFormId;
use soe_game::dialogue::Line;

pub(super) struct MenuView {
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    content: Gd<VBoxContainer>,
    close: Gd<Button>,
    previous_mouse: Option<MouseMode>,
    previous_focus: Option<Gd<Control>>,
}

impl PlayerNode {
    fn stop_dialogue_voice(&mut self) {
        if let Some(bank) = self.sound_bank.as_mut() {
            crate::dialogue_menu::replace_voice(
                &mut *bank.bind_mut(),
                &mut self.dialogue_voice_slot,
                None,
            );
        } else {
            self.dialogue_voice_slot = None;
        }
    }

    fn play_dialogue_voice(&mut self, line: &Line) {
        let asset = self
            .game
            .as_mut()
            .and_then(|game| crate::dialogue_menu::voice_asset(game, line));
        let Some(bank) = self.sound_bank.as_mut() else {
            return;
        };
        crate::dialogue_menu::replace_voice(
            &mut *bank.bind_mut(),
            &mut self.dialogue_voice_slot,
            asset.as_ref(),
        );
    }

    pub(super) fn create_menu(&mut self) {
        let mut theme = Theme::new_gd();
        theme.set_default_font_size(22);
        theme.set_color("font_color", "Label", Color::from_rgb(0.94, 0.94, 0.96));
        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(Color::from_rgba(0.06, 0.07, 0.09, 0.98));
        style.set_content_margin_all(32.0);
        theme.set_stylebox("panel", "PanelContainer", &style);

        let mut canvas = CanvasLayer::new_alloc();
        canvas.set_name("MenuLayer");
        canvas.set_layer(100);
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("MenuPanel");
        panel.set_theme(&theme);
        panel.hide();
        let mut layout = VBoxContainer::new_alloc();
        layout.set_name("Layout");
        let mut title = Label::new_alloc();
        title.set_name("Title");
        let mut content = VBoxContainer::new_alloc();
        content.set_name("Content");
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut close = Button::new_alloc();
        close.set_name("Close");
        close.set_text("Close");
        close.connect("pressed", &self.base().callable("close_menu"));
        layout.add_child(&title);
        layout.add_child(&content);
        layout.add_child(&close);
        panel.add_child(&layout);
        canvas.add_child(&panel);
        self.base_mut().add_child(&canvas);
        panel.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.menu_view = Some(MenuView {
            panel,
            title,
            content,
            close,
            previous_mouse: None,
            previous_focus: None,
        });
    }

    /// Shared form-backed game access for native menu presenters.
    pub fn game(&self) -> Option<&soe_game::Game> {
        self.game.as_ref()
    }

    /// Mutations from a menu use the same Game as movement, scripts and activation.
    pub fn game_mut(&mut self) -> Option<&mut soe_game::Game> {
        self.game.as_mut()
    }

    fn crafting_content(&mut self, bench: GlobalFormId) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let actor = game.player();
        let entries = crafting_menu::entries(game, actor, bench);
        let recipe_rows: Vec<_> = entries
            .into_iter()
            .map(|entry| {
                let output = game
                    .world()
                    .name_of(entry.output)
                    .unwrap_or_else(|| format!("{}", entry.output));
                (entry, output)
            })
            .collect();
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        scroll.add_child(&rows);
        content.add_child(&scroll);
        let mut first_available: Option<Gd<Button>> = None;
        if recipe_rows.is_empty() {
            let mut empty = Label::new_alloc();
            empty.set_text("No recipes available");
            rows.add_child(&empty);
        }
        for (entry, output) in recipe_rows {
            let mut button = Button::new_alloc();
            button.set_text(&format!("{} → {} x{}", entry.name, output, entry.count));
            button.set_disabled(!entry.available);
            let callback = base.callable("confirm_crafting_recipe").bindv(&varray![
                i64::from(bench.file),
                i64::from(bench.object),
                i64::from(entry.recipe.file),
                i64::from(entry.recipe.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
            if entry.available && first_available.is_none() {
                first_available = Some(button.clone());
            }
        }
        if let Some(mut button) = first_available {
            button.grab_focus();
        }
        true
    }

    fn inventory_content(&mut self, category: Category) -> bool {
        if self.menu.title() != Some("Inventory") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let player = game.player();
        let entries = inventory_menu::entries(game, player, category);
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        scroll.add_child(&rows);
        content.add_child(&scroll);
        for (index, category) in Category::ALL.into_iter().enumerate() {
            let mut button = Button::new_alloc();
            button.set_text(category.label());
            let callback = base
                .callable("inventory_category")
                .bindv(&varray![index as i64]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
            if index == 0 {
                button.grab_focus();
            }
        }
        let mut heading = Label::new_alloc();
        heading.set_text(&format!("{} ({})", category.label(), entries.len()));
        rows.add_child(&heading);
        for entry in entries {
            let mut button = Button::new_alloc();
            let equipped = if entry.equipped { " [Equipped]" } else { "" };
            button.set_text(&format!("{} x{}{}", entry.name, entry.count, equipped));
            let callback = base.callable("toggle_inventory_item").bindv(&varray![
                i64::from(entry.item.file),
                i64::from(entry.item.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        true
    }

    fn dialogue_content(&mut self) -> bool {
        if self.menu.title() != Some("Dialogue") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let Ok(entries) = dialogue_menu::entries(game) else {
            return false;
        };
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        scroll.add_child(&rows);
        content.add_child(&scroll);
        for entry in entries {
            let mut button = Button::new_alloc();
            button.set_text(&entry.label);
            let callback = base.callable("select_dialogue_topic").bindv(&varray![
                i64::from(entry.topic.file),
                i64::from(entry.topic.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
            if rows.get_child_count() == 1 {
                button.grab_focus();
            }
        }
        if rows.get_child_count() == 0 {
            let mut empty = Label::new_alloc();
            empty.set_text("No available topics.");
            rows.add_child(&empty);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Open the modal shell. The framework owns controls added to menu_content.
    #[func]
    pub fn open_menu(&mut self, title: GString) -> bool {
        if !self.base().is_inside_tree() || self.menu_view.is_none() {
            return false;
        }
        if !self.menu.open(&title.to_string()) {
            return false;
        }
        self.container_menu = None;
        self.crafting_bench = None;
        let previous_focus = self
            .base()
            .get_viewport()
            .and_then(|v| v.gui_get_focus_owner());
        let focused = self.input_focused;
        let previous_mouse = self
            .restore_mouse_on_focus
            .take()
            .unwrap_or_else(|| Input::singleton().get_mouse_mode());
        self.input_focus_lost();
        if focused {
            self.input_focus_gained();
        }
        let presentation = self.menu_view.as_mut().map(|view| {
            view.previous_mouse = Some(previous_mouse);
            view.previous_focus = previous_focus;
            (view.title.clone(), view.panel.clone(), view.close.clone())
        });
        Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
        if let Some((mut label, mut panel, mut close)) = presentation {
            // CONTEXT: Focus and visibility callbacks may query the player.
            let _guard = self.base_mut();
            label.set_text(&title);
            panel.show();
            close.grab_focus();
        }
        self.base_mut()
            .emit_signal("menu_opened", &[Variant::from(title)]);
        true
    }

    /// Close once, discard queued gameplay presses, and restore focus and mouse mode.
    #[func]
    pub fn close_menu(&mut self) -> bool {
        let closing_dialogue = self.menu.title() == Some("Dialogue");
        if !self.menu.close() {
            return false;
        }
        self.crafting_bench = None;
        if closing_dialogue {
            self.stop_dialogue_voice();
            if let Some(game) = self.game.as_mut() {
                game.close_dialogue();
            }
        }
        let focused = self.input_focused;
        self.input_focus_lost();
        if focused {
            self.input_focus_gained();
        }
        let presentation = self.menu_view.as_mut().map(|view| {
            (
                view.panel.clone(),
                view.content.clone(),
                view.previous_mouse.take(),
                view.previous_focus.take(),
            )
        });
        if let Some((mut panel, mut content, mouse, focus)) = presentation {
            if let Some(mode) = mouse {
                if focused {
                    Input::singleton().set_mouse_mode(mode);
                } else {
                    self.restore_mouse_on_focus = Some(mode);
                }
            }
            // CONTEXT: Presenter teardown and focus callbacks may query the player.
            let _guard = self.base_mut();
            if panel.is_instance_valid() {
                panel.hide();
            }
            if content.is_instance_valid() {
                for mut child in content.get_children().iter_shared() {
                    content.remove_child(&child);
                    child.queue_free();
                }
            }
            if let Some(mut focus) = focus {
                if focus.is_instance_valid() && focus.is_inside_tree() && focus.is_visible_in_tree()
                {
                    focus.grab_focus();
                }
            }
        }
        self.base_mut().emit_signal("menu_closed", &[]);
        true
    }

    #[func]
    pub fn is_menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// A native Control host for the current menu's presenter. None while closed.
    #[func]
    pub fn menu_content(&self) -> Option<Gd<Control>> {
        self.menu
            .is_open()
            .then_some(self.menu_view.as_ref())
            .flatten()
            .map(|view| view.content.clone().upcast())
    }

    /// Read the shared world clock for UI displays and host integration checks.
    #[func]
    pub fn gameplay_days(&self) -> f64 {
        self.game
            .as_ref()
            .map_or(0.0, |game| game.world().clock.game_days)
    }

    /// Open the shared player's inventory and render one SkyUI-style category.
    #[func]
    pub fn open_inventory_menu(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Inventory".into()) {
            return false;
        }
        self.inventory_category = Category::All;
        self.inventory_content(self.inventory_category)
    }

    /// Open the recipe list for an activated creation bench.
    #[func]
    pub fn open_crafting_menu(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let bench = GlobalFormId { file, object };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        if !crafting_menu::is_creation_bench(game, bench) {
            return false;
        }
        let title = game
            .world()
            .name_of(bench)
            .unwrap_or_else(|| "Crafting".into());
        if !self.open_menu(GString::from(&title)) {
            return false;
        }
        self.crafting_bench = Some(bench);
        self.crafting_content(bench)
    }

    /// Confirm a recipe from the currently open bench menu through `Game::craft`.
    #[func]
    pub fn confirm_crafting_recipe(
        &mut self,
        bench_file: i64,
        bench_object: i64,
        recipe_file: i64,
        recipe_object: i64,
    ) -> bool {
        let (Ok(bench_file), Ok(bench_object), Ok(recipe_file), Ok(recipe_object)) = (
            u16::try_from(bench_file),
            u32::try_from(bench_object),
            u16::try_from(recipe_file),
            u32::try_from(recipe_object),
        ) else {
            return false;
        };
        let bench = GlobalFormId {
            file: bench_file,
            object: bench_object,
        };
        let recipe = GlobalFormId {
            file: recipe_file,
            object: recipe_object,
        };
        if !self.menu.is_open() || self.crafting_bench != Some(bench) {
            return false;
        }
        let crafted = self.game.as_mut().is_some_and(|game| {
            let actor = game.player();
            crafting_menu::confirm(game, actor, bench, recipe).is_ok()
        });
        if crafted {
            self.crafting_content(bench)
        } else {
            false
        }
    }

    fn dialogue_response_content(&mut self, text: &str) -> bool {
        if self.menu.title() != Some("Dialogue") {
            return false;
        }
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut response = Label::new_alloc();
        response.set_text(text);
        content.add_child(&response);
        let mut next = Button::new_alloc();
        next.set_text("Continue");
        next.connect("pressed", &base.callable("advance_dialogue_menu"));
        content.add_child(&next);
        next.grab_focus();
        true
    }

    /// Open a conversation with a reference through the shared Game.
    #[func]
    pub fn open_dialogue_menu(&mut self, speaker_file: i64, speaker_object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(speaker_file), u32::try_from(speaker_object))
        else {
            return false;
        };
        if self.game.is_none()
            || self.menu.is_open()
            || !self.base().is_inside_tree()
            || self.menu_view.is_none()
        {
            return false;
        }
        let speaker = GlobalFormId { file, object };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let listener = game.player();
        if game.begin_dialogue(speaker, listener).is_err() {
            return false;
        }
        if !self.open_menu("Dialogue".into()) {
            if let Some(game) = self.game.as_mut() {
                game.close_dialogue();
            }
            return false;
        }
        if self.dialogue_content() {
            true
        } else {
            self.close_menu();
            false
        }
    }

    /// Select an offered topic through Game and show its response.
    #[func]
    pub fn select_dialogue_topic(&mut self, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Dialogue") {
            return false;
        }
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let topic = GlobalFormId { file, object };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let Ok(Some(line)) = dialogue_menu::choose(game, topic) else {
            return false;
        };
        self.play_dialogue_voice(&line);
        self.dialogue_response_content(line.text.as_deref().unwrap_or(""))
    }

    /// Advance a response and show the next response or topic choices.
    #[func]
    pub fn advance_dialogue_menu(&mut self) -> bool {
        if self.menu.title() != Some("Dialogue") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        match game.advance_dialogue() {
            Ok(Some(line)) => {
                self.play_dialogue_voice(&line);
                self.dialogue_response_content(line.text.as_deref().unwrap_or(""))
            }
            Ok(None) => {
                self.stop_dialogue_voice();
                let still_open = self
                    .game
                    .as_ref()
                    .is_some_and(|game| game.world().dialogue_session().is_some());
                if still_open {
                    self.dialogue_content()
                } else {
                    self.close_menu()
                }
            }
            Err(_) => false,
        }
    }

    /// Select one of [`Category::ALL`] in the open inventory menu.
    #[func]
    pub fn inventory_category(&mut self, index: i64) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let Some(category) = Category::ALL.get(index).copied() else {
            return false;
        };
        self.inventory_category = category;
        self.inventory_content(category)
    }

    /// Toggle a listed weapon or armor through the shared game state, then refresh the list.
    #[func]
    pub fn toggle_inventory_item(&mut self, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Inventory") {
            return false;
        }
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let item = GlobalFormId { file, object };
        let changed = self
            .game
            .as_mut()
            .is_some_and(|game| inventory_menu::toggle_equip(game, item));
        if changed {
            self.inventory_content(self.inventory_category);
        }
        changed
    }
}

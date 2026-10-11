// SPDX-License-Identifier: GPL-3.0-or-later

//! Native modal UI from Godot Control/Theme and gdext node ownership examples.
//! PlayerNode keeps ticking the shared Game while this fixed control tree is open.

use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::input::MouseMode;
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, Input, Label, PanelContainer, ScrollContainer,
    StyleBoxFlat, Theme, VBoxContainer,
};
use godot::prelude::*;

use super::PlayerNode;
use crate::crafting_menu;
use crate::dialogue_menu;
use crate::inventory_menu::{self, Category};
use crate::objective_map;
use crate::quest_journal::{self, Entry as QuestEntry};
use soe_esm::GlobalFormId;
use soe_game::dialogue::Line;

fn render_quest_entry(rows: &mut Gd<VBoxContainer>, entry: &QuestEntry) {
    if let Some(title) = &entry.title {
        let mut heading = Label::new_alloc();
        heading.set_text(title);
        rows.add_child(&heading);
    }
    for objective in &entry.objectives {
        let status = if objective.failed {
            " [Failed]"
        } else if objective.completed {
            " [Completed]"
        } else {
            ""
        };
        let mut label = Label::new_alloc();
        label.set_text(&format!("{}{}", objective.text, status));
        rows.add_child(&label);
    }
}

pub(super) struct MenuView {
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    content: Gd<VBoxContainer>,
    close: Gd<Button>,
    previous_mouse: Option<MouseMode>,
    previous_focus: Option<Gd<Control>>,
}

impl PlayerNode {
    pub(super) fn item_panel_layout(&mut self) {
        if let Some(view) = self.menu_view.as_mut() {
            use godot::builtin::Side;
            view.panel
                .set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            for (side, anchor) in [
                (Side::LEFT, 0.04),
                (Side::TOP, 0.08),
                (Side::RIGHT, 0.96),
                (Side::BOTTOM, 0.92),
            ] {
                view.panel
                    .set_anchor_ex(side, anchor)
                    .keep_offset(true)
                    .done();
            }
        }
    }

    pub(super) fn advance_ambient_dialogue(&mut self) {
        if self.menu.title() == Some("Dialogue") {
            return;
        }
        let ambient = self
            .game
            .as_ref()
            .is_some_and(|game| game.ambient_dialogue());
        if !ambient {
            if self.ambient_response.take().is_some() {
                self.stop_dialogue_voice();
            }
            return;
        }
        let line = self
            .game
            .as_ref()
            .and_then(|game| game.dialogue_line().ok().flatten());
        if line != self.ambient_response {
            if let Some(line) = &line {
                self.play_dialogue_voice(line);
            }
            self.ambient_response = line;
            return;
        }
        let playing = self.dialogue_voice_slot.is_some_and(|slot| {
            self.sound_bank
                .as_ref()
                .is_some_and(|bank| bank.bind().voice_playing(slot))
        });
        if !playing {
            let next = self.game.as_mut().and_then(|game| {
                match game.advance_dialogue() {
                    Ok(Some(line)) => Some(line),
                    Ok(None) => { game.close_dialogue(); None }
                    Err(error) => {
                        soe_trace::error!(target: "host::audio", system = "host::audio", "ambient dialogue: {error}");
                        game.close_dialogue();
                        None
                    }
                }
            });
            if let Some(line) = &next {
                self.play_dialogue_voice(line);
            } else {
                self.stop_dialogue_voice();
            }
            self.ambient_response = next;
        }
    }

    pub(super) fn present_player_input_dialogue(&mut self, activation: bool) {
        let greeting = self
            .game
            .as_ref()
            .is_some_and(|game| game.ambient_dialogue());
        if greeting && !activation {
            return;
        }
        if self.ambient_response.take().is_some() {
            self.stop_dialogue_voice();
        }
        let line = self
            .game
            .as_ref()
            .and_then(|game| game.dialogue_line().ok());
        let session = self
            .game
            .as_ref()
            .is_some_and(|game| game.world().dialogue_session().is_some());
        if !session {
            if self.menu.title() == Some("Dialogue") {
                self.close_menu();
            }
            return;
        }
        if !self.menu.is_open() {
            self.open_menu("Dialogue".into());
        }
        if self.menu.title() != Some("Dialogue") {
            return;
        }
        if let Some(Some(line)) = line {
            self.play_dialogue_voice(&line);
            self.dialogue_response_content(line.text.as_deref().unwrap_or(""));
            if greeting {
                self.greeting_topic_content();
            }
        } else {
            self.dialogue_content();
        }
    }

    pub(super) fn refresh_player_input_crafting(&mut self, bench: GlobalFormId) {
        if self.crafting_bench == Some(bench) {
            self.crafting_content(bench);
        }
    }

    pub(super) fn refresh_quest_journal(&mut self) -> bool {
        if self.menu.title() != Some("Quests") {
            return false;
        }
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let entries = quest_journal::entries(game);
        if self.quest_journal_snapshot.as_ref() == Some(&entries) {
            return false;
        }
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let base = self.base().clone();
        self.quest_journal_snapshot = Some(entries.clone());
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        scroll.add_child(&rows);
        let mut map = Button::new_alloc();
        map.set_name("OpenObjectiveMap");
        map.set_text("Map");
        map.connect("pressed", &base.callable("open_objective_map"));
        content.add_child(&map);
        content.add_child(&scroll);
        if entries.is_empty() {
            let mut label = Label::new_alloc();
            label.set_text("No active quests");
            rows.add_child(&label);
        }
        for entry in &entries {
            render_quest_entry(&mut rows, entry);
        }
        true
    }

    /// Rebuild the map surface when the shared objective targets change.
    pub(super) fn refresh_objective_map(&mut self) -> bool {
        if self.menu.title() != Some("Map") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let markers = objective_map::markers(game);
        if self.objective_map_snapshot.as_ref() == Some(&markers) {
            return false;
        }
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        self.objective_map_snapshot = Some(markers.clone());
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut surface = Control::new_alloc();
        surface.set_name("ObjectiveMapSurface");
        surface.set_custom_minimum_size(Vector2::new(640.0, 360.0));
        surface.set_h_size_flags(SizeFlags::EXPAND_FILL);
        surface.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut background = ColorRect::new_alloc();
        background.set_name("MapBackground");
        background.set_color(Color::from_rgb(0.10, 0.14, 0.13));
        surface.add_child(&background);
        background.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        for point in objective_map::project(&markers, 640.0, 360.0) {
            let mut label = Label::new_alloc();
            label.set_name(&format!(
                "Objective_{}_{}",
                point.objective, point.reference.object
            ));
            label.set_text(&format!("◆ {}", point.objective));
            label.set_position(Vector2::new(point.x, point.y));
            label.set_visible(point.visible);
            surface.add_child(&label);
        }
        content.add_child(&surface);
        true
    }

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
        if asset.is_some() && self.dialogue_voice_slot.is_none() {
            soe_trace::error!(target: "host::audio", system = "host::audio", "dialogue voice: {}", bank.bind().last_error());
        }
    }

    pub(super) fn create_menu(&mut self) {
        let mut theme = Theme::new_gd();
        theme.set_default_font_size(16);
        theme.set_color("font_color", "Label", Color::from_rgb(0.94, 0.94, 0.96));
        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(Color::from_rgba(0.06, 0.07, 0.09, 0.98));
        style.set_content_margin_all(12.0);
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
                let output = entry.name.clone();
                (entry, output)
            })
            .collect();
        let Some(mut content) = self
            .menu_view
            .as_ref()
            .map(|view| view.content.clone().upcast::<Control>())
        else {
            return false;
        };
        let mut rows = self.item_menu_rows(
            &mut content,
            recipe_rows.first().map(|(entry, _)| entry.output),
        );
        let base = self.base().clone();
        let _guard = self.base_mut();
        let mut first_available: Option<Gd<Button>> = None;
        if recipe_rows.is_empty() {
            let mut empty = Label::new_alloc();
            empty.set_text("No recipes available");
            rows.add_child(&empty);
        }
        for (entry, output) in recipe_rows {
            let mut button = Button::new_alloc();
            button.set_text(&format!("{} x{}", output, entry.count));
            button.set_disabled(!entry.available);
            let callback = base.callable("confirm_crafting_recipe").bindv(&varray![
                i64::from(bench.file),
                i64::from(bench.object),
                i64::from(entry.recipe.file),
                i64::from(entry.recipe.object),
            ]);
            super::item_card::connect_preview(&base, &mut button, entry.output);
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
        if !self.present_inventory_items() {
            return false;
        }
        if let Some(mut view) = self
            .menu_content()
            .and_then(|content| content.get_node_or_null("ItemMenu"))
        {
            let _guard = self.base_mut();
            view.call("select_category", &[category.label().to_variant()]);
        }
        true
    }

    fn mcm_content(&mut self) -> bool {
        if self.menu.title() != Some("Mod Configuration") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let configs = game.mcm_configs();
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return false;
        };
        let selected = self.mcm_config;
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
        for config in &configs {
            let mut button = Button::new_alloc();
            button.set_text(&config.name);
            let callback = base.callable("mcm_select_config").bindv(&varray![
                i64::from(config.form.file),
                i64::from(config.form.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        if let Some(config) = configs.iter().find(|config| Some(config.form) == selected) {
            let mut heading = Label::new_alloc();
            heading.set_text(&config.name);
            rows.add_child(&heading);
            for (index, page) in config.pages.iter().enumerate() {
                let mut button = Button::new_alloc();
                button.set_text(page);
                let callback = base
                    .callable("mcm_select_page")
                    .bindv(&varray![index as i64]);
                button.connect("pressed", &callback);
                rows.add_child(&button);
            }
        }
        true
    }

    fn greeting_topic_content(&mut self) {
        let entries = self
            .game
            .as_mut()
            .and_then(|game| dialogue_menu::greeting_entries(game).ok())
            .unwrap_or_default();
        let Some(mut content) = self.menu_view.as_ref().map(|view| view.content.clone()) else {
            return;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        for entry in entries {
            let mut button = Button::new_alloc();
            button.set_text(&entry.label);
            button.connect(
                "pressed",
                &base.callable("select_dialogue_topic").bindv(&varray![
                    i64::from(entry.topic.file),
                    i64::from(entry.topic.object),
                ]),
            );
            content.add_child(&button);
        }
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
    /// Current record-backed conversation and choices for host diagnostics.
    #[func]
    pub fn dialogue_state(&mut self) -> VarDictionary {
        let mut state = VarDictionary::new();
        let Some(game) = &mut self.game else {
            return state;
        };
        let Some(session) = game.world().dialogue_session().cloned() else {
            return state;
        };
        state.set("speaker_file", i64::from(session.speaker.file));
        state.set("speaker_object", i64::from(session.speaker.object));
        state.set("ambient", game.ambient_dialogue());
        state.set("available", game.player_dialogue_available(session.speaker));
        if let Ok(Some(line)) = game.dialogue_line() {
            state.set("text", line.text.as_deref().unwrap_or(""));
            state.set("info_object", i64::from(line.info.object));
        }
        let mut topics = Array::<VarDictionary>::new();
        if let Ok(entries) = dialogue_menu::greeting_entries(game) {
            for entry in entries {
                topics.push(&vdict! { "file" => i64::from(entry.topic.file), "object" => i64::from(entry.topic.object), "label" => entry.label.as_str() });
            }
        }
        state.set("topics", &topics);
        state.set("voice_slot", self.dialogue_voice_slot.unwrap_or(-1));
        state
    }

    /// Select and play one eligible MUSC track for the current player.
    #[func]
    pub fn select_music_type(&mut self, file: i64, object: i64, draw: f64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let track = {
            let Some(game) = self.game.as_mut() else {
                return false;
            };
            let music_type = GlobalFormId { file, object };
            let subject = game.player();
            match soe_game::music::select(game, music_type, subject, draw as f32) {
                Ok(track) => track,
                Err(_) => return false,
            }
        };
        self.sound_bank
            .as_mut()
            .is_some_and(|bank| bank.bind_mut().play_selected_music(&track))
    }

    /// Open the modal shell. The framework owns controls added to menu_content.
    #[func]
    pub fn open_menu(&mut self, title: GString) -> bool {
        if !self.base().is_inside_tree() || self.menu_view.is_none() {
            return false;
        }
        if !self.menu.open(&title.to_string()) {
            return false;
        }
        self.hide_quick_loot();
        self.base_mut().emit_signal(
            "transition_requested",
            &[Variant::from("menu_open"), Variant::from(title.clone())],
        );
        self.container_menu = None;
        self.barter_menu = None;
        self.crafting_bench = None;
        self.item_card_view = None;
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
            panel.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
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
        if let Some(title) = self.menu.title().map(str::to_owned) {
            self.base_mut().emit_signal(
                "transition_requested",
                &[Variant::from("menu_close"), Variant::from(title.as_str())],
            );
        }
        let closing_dialogue = self.menu.title() == Some("Dialogue");
        let closing_gift = self
            .menu
            .title()
            .is_some_and(|title| title.starts_with("Gift: "));
        if self.menu.title() == Some("Race") {
            self.race_choice = None;
            crate::race_menu::discard_draft(&mut self.appearance_choice);
        }
        if !self.menu.close() {
            return false;
        }
        self.item_card_view = None;
        self.crafting_bench = None;
        self.lockpick_target = None;
        self.lockpick_status = None;
        if let Some(game) = self.game.as_mut() {
            game.mcm_close();
        }
        self.mcm_config = None;
        if closing_dialogue {
            self.stop_dialogue_voice();
            if let Some(game) = self.game.as_mut() {
                game.close_dialogue();
            }
        }
        if closing_gift {
            if let Some(game) = self.game.as_mut() {
                game.close_gift_menu();
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

    /// Open active quests and displayed objectives from the shared Game.
    #[func]
    pub fn open_quest_journal(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Quests".into()) {
            return false;
        }
        self.quest_journal_snapshot = None;
        self.refresh_quest_journal()
    }

    /// Set a quest stage in the shared game state.
    #[func]
    pub fn set_quest_stage(&mut self, file: i64, object: i64, stage: i64) -> bool {
        let (Ok(file), Ok(object), Ok(stage)) = (
            u16::try_from(file),
            u32::try_from(object),
            u16::try_from(stage),
        ) else {
            return false;
        };
        self.game
            .as_mut()
            .is_some_and(|game| game.set_stage(GlobalFormId { file, object }, stage))
    }

    /// Open the native map surface with markers from the shared quest state.
    #[func]
    pub fn open_objective_map(&mut self) -> bool {
        if self.game.is_none() || self.menu.title() != Some("Quests") {
            return false;
        }
        self.close_menu();
        if !self.open_menu("Map".into()) {
            return false;
        }
        self.objective_map_snapshot = None;
        self.refresh_objective_map()
    }

    /// Open the native mod configuration list from live quest scripts.
    #[func]
    pub fn open_mcm_menu(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Mod Configuration".into()) {
            return false;
        }
        self.mcm_config = None;
        self.mcm_content()
    }

    /// Open one registered script config and render its declared pages.
    #[func]
    pub fn mcm_select_config(&mut self, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Mod Configuration") {
            return false;
        }
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let form = GlobalFormId { file, object };
        if !self.game.as_mut().is_some_and(|game| game.mcm_open(form)) {
            return false;
        }
        self.mcm_config = Some(form);
        self.mcm_content()
    }

    /// Select one of the active script's declared pages.
    #[func]
    pub fn mcm_select_page(&mut self, index: i64) -> bool {
        if self.menu.title() != Some("Mod Configuration") {
            return false;
        }
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        self.game
            .as_mut()
            .is_some_and(|game| game.mcm_select_page(index))
    }

    /// Open a conversation with a reference through the shared Game and show its choices.
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
        self.base_mut().emit_signal(
            "transition_requested",
            &[
                Variant::from("dialogue_activation"),
                Variant::from("topics"),
            ],
        );
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let Ok(greeting) = game.begin_player_dialogue(speaker) else {
            return false;
        };
        if self.ambient_response.take().is_some() {
            self.stop_dialogue_voice();
        }
        if !self.open_menu("Dialogue".into()) {
            if let Some(game) = self.game.as_mut() {
                game.close_dialogue();
            }
            return false;
        }
        let presented = if let Some(line) = greeting {
            self.play_dialogue_voice(&line);
            let shown = self.dialogue_response_content(line.text.as_deref().unwrap_or(""));
            if shown {
                self.greeting_topic_content();
            }
            shown
        } else {
            self.dialogue_content()
        };
        if presented {
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
        if game.ambient_dialogue() {
            let eligible = game
                .dialogue_choices_after_response()
                .is_ok_and(|choices| choices.available.iter().any(|choice| choice.topic == topic));
            if !eligible {
                return false;
            }
            let mut visited = std::collections::BTreeSet::new();
            loop {
                let cursor = game.world().dialogue_session().and_then(|session| {
                    session
                        .current_info
                        .map(|info| (info, session.response_index))
                });
                if !visited.insert(cursor) {
                    return false;
                }
                match game.advance_dialogue() {
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                    Err(_) => return false,
                }
            }
        }
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

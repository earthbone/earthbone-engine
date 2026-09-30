// SPDX-License-Identifier: GPL-3.0-or-later

//! Native race and sex selection presenter for the shared player world.

use godot::classes::{Button, Label, VBoxContainer};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;
use crate::race_menu;

impl PlayerNode {
    fn race_content(&mut self) -> bool {
        if self.menu.title() != Some("Race") {
            return false;
        }
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let entries = race_menu::entries(game);
        let parts = self.race_choice.map_or_else(Vec::new, |(race, female)| {
            race_menu::head_parts(game, race, female)
        });
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        let base = self.base().clone();
        let choice = self.race_choice;
        let appearance = self.appearance_choice.clone().unwrap_or_default();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut rows = VBoxContainer::new_alloc();
        content.add_child(&rows);
        let mut heading = Label::new_alloc();
        heading.set_text("Choose a race and sex");
        rows.add_child(&heading);
        let mut preview = Label::new_alloc();
        preview.set_text(&race_menu::preview_summary(&appearance, &parts));
        rows.add_child(&preview);
        for entry in entries {
            let mut button = Button::new_alloc();
            let selected = choice.is_some_and(|(race, _)| race == entry.race);
            button.set_text(&format!(
                "{}{}",
                entry.name,
                if selected { " *" } else { "" }
            ));
            let callback = base.callable("select_player_race").bindv(&varray![
                i64::from(entry.race.file),
                i64::from(entry.race.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        for (female, label) in [(false, "Male"), (true, "Female")] {
            let mut button = Button::new_alloc();
            let selected = choice.is_some_and(|(_, current)| current == female);
            button.set_text(&format!("{}{}", label, if selected { " *" } else { "" }));
            let callback = base.callable("select_player_sex").bindv(&varray![female]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        for entry in parts {
            let mut button = Button::new_alloc();
            let selected = appearance.head_parts.get(&entry.part_type) == Some(&entry.form);
            button.set_text(&format!(
                "{}{}",
                entry.name,
                if selected { " *" } else { "" }
            ));
            let callback = base.callable("select_player_head_part").bindv(&varray![
                i64::from(entry.part_type),
                i64::from(entry.form.file),
                i64::from(entry.form.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        for name in ["Face", "Nose", "Jaw"] {
            let value = appearance.morphs.get(name).copied().unwrap_or(0.0);
            for (step, label) in [(-0.1_f32, "-"), (0.1_f32, "+")] {
                let mut button = Button::new_alloc();
                button.set_text(&format!("{name} {label} ({value:.2})"));
                let callback = base
                    .callable("adjust_player_morph")
                    .bindv(&varray![name, step]);
                button.connect("pressed", &callback);
                rows.add_child(&button);
            }
        }
        let mut confirm = Button::new_alloc();
        confirm.set_text("Confirm");
        confirm.connect("pressed", &base.callable("confirm_player_character"));
        rows.add_child(&confirm);
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Open the record-backed race and sex menu.
    #[func]
    pub fn show_race_menu(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Race".into()) {
            return false;
        }
        let current = self
            .game
            .as_mut()
            .and_then(|game| game.world_mut().player_character_traits());
        let Some(current) = current else {
            self.close_menu();
            return false;
        };
        self.race_choice = Some(current);
        self.appearance_choice = self
            .game
            .as_mut()
            .and_then(|game| game.world_mut().player_appearance());
        if self.race_content() {
            true
        } else {
            self.close_menu();
            false
        }
    }

    /// Select a winning playable race record for the player.
    #[func]
    pub fn select_player_race(&mut self, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Race") {
            return false;
        }
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let race = GlobalFormId { file, object };
        if !self.game.as_ref().is_some_and(|game| {
            race_menu::entries(game)
                .iter()
                .any(|entry| entry.race == race)
        }) {
            return false;
        }
        let Some((_, female)) = self.race_choice else {
            return false;
        };
        self.race_choice = Some((race, female));
        self.retain_supported_head_parts();
        self.race_content()
    }

    /// Select the player's sex, retaining the currently selected race.
    #[func]
    pub fn select_player_sex(&mut self, female: bool) -> bool {
        if self.menu.title() != Some("Race") {
            return false;
        }
        let Some((race, _)) = self.race_choice else {
            return false;
        };
        self.race_choice = Some((race, female));
        self.retain_supported_head_parts();
        self.race_content()
    }

    fn retain_supported_head_parts(&mut self) {
        let Some((race, female)) = self.race_choice else {
            return;
        };
        let supported = self.game.as_ref().map_or_else(Vec::new, |game| {
            race_menu::head_parts(game, race, female)
                .into_iter()
                .map(|entry| (entry.part_type, entry.form))
                .collect::<Vec<_>>()
        });
        if let Some(appearance) = self.appearance_choice.as_mut() {
            appearance
                .head_parts
                .retain(|part_type, form| supported.contains(&(*part_type, *form)));
        }
    }

    /// Select a compatible playable head part in the open race menu.
    #[func]
    pub fn select_player_head_part(&mut self, part_type: i64, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Race") {
            return false;
        }
        let (Ok(part_type), Ok(file), Ok(object)) = (
            u32::try_from(part_type),
            u16::try_from(file),
            u32::try_from(object),
        ) else {
            return false;
        };
        let form = GlobalFormId { file, object };
        let Some((race, female)) = self.race_choice else {
            return false;
        };
        if !self.game.as_ref().is_some_and(|game| {
            race_menu::head_parts(game, race, female)
                .iter()
                .any(|entry| entry.part_type == part_type && entry.form == form)
        }) {
            return false;
        }
        let Some(appearance) = self.appearance_choice.as_mut() else {
            return false;
        };
        appearance.head_parts.insert(part_type, form);
        self.race_content()
    }

    /// Adjust a normalized face morph by one menu step.
    #[func]
    #[allow(clippy::needless_pass_by_value)] // Godot callbacks receive owned Variants.
    pub fn adjust_player_morph(&mut self, name: GString, step: f32) -> bool {
        if self.menu.title() != Some("Race") || ![-0.1, 0.1].contains(&step) {
            return false;
        }
        let Some(appearance) = self.appearance_choice.as_mut() else {
            return false;
        };
        let key = name.to_string();
        if !["Face", "Nose", "Jaw"].contains(&key.as_str()) {
            return false;
        }
        let value = (appearance.morphs.get(&key).copied().unwrap_or(0.0) + step).clamp(-1.0, 1.0);
        appearance.morphs.insert(key, value);
        self.race_content()
    }

    /// Apply selected traits, initialize actor values on first access, and close the menu.
    #[func]
    pub fn confirm_player_character(&mut self) -> bool {
        if self.menu.title() != Some("Race") {
            return false;
        }
        let Some((race, female)) = self.race_choice else {
            return false;
        };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let player = game.player();
        let Some(appearance) = self.appearance_choice.as_ref() else {
            return false;
        };
        for (part_type, part) in &appearance.head_parts {
            if game
                .world()
                .validate_player_head_part(*part_type, *part, female)
                .is_err()
            {
                return false;
            }
        }
        if appearance.morphs.iter().any(|(name, value)| {
            !["Face", "Nose", "Jaw"].contains(&name.as_str())
                || name.trim().is_empty()
                || !value.is_finite()
                || !(-1.0..=1.0).contains(value)
        }) {
            return false;
        }
        if game
            .world_mut()
            .set_player_character_traits(race, female)
            .is_err()
        {
            return false;
        }
        for (part_type, part) in &appearance.head_parts {
            if game
                .world_mut()
                .set_player_head_part(*part_type, *part)
                .is_err()
            {
                return false;
            }
        }
        for (name, value) in &appearance.morphs {
            if game
                .world_mut()
                .set_player_face_morph(name, *value)
                .is_err()
            {
                return false;
            }
        }
        game.world_mut().actor(player);
        self.race_choice = None;
        self.appearance_choice = None;
        self.close_menu()
    }
}

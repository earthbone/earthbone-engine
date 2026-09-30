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
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        let base = self.base().clone();
        let choice = self.race_choice;
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
        if game
            .world_mut()
            .set_player_character_traits(race, female)
            .is_err()
        {
            return false;
        }
        game.world_mut().actor(player);
        self.race_choice = None;
        self.close_menu()
    }
}

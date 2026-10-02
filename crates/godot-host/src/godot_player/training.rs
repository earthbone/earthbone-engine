// SPDX-License-Identifier: GPL-3.0-or-later

//! Native trainer menu presenter over the shared Game.

use crate::training_menu;
use godot::classes::control::SizeFlags;
use godot::classes::{Button, Label, ScrollContainer, VBoxContainer};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;

impl PlayerNode {
    fn training_content(&mut self) -> bool {
        if self.menu.title() != Some("Training") {
            return false;
        }
        let Some(trainer) = self.training_trainer else {
            return false;
        };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let entry = training_menu::entry(game, trainer);
        let player = game.player();
        let can_pay = entry.as_ref().is_some_and(|entry| {
            game.world_mut().item_count(
                player,
                soe_esm::GlobalFormId {
                    file: 0,
                    object: 0xF,
                },
            ) >= entry.cost
                && entry.current < entry.cap
                && entry.sessions_remaining > 0
        });
        let Some(mut content) = self.menu_content() else {
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
        if let Some(entry) = entry {
            let mut heading = Label::new_alloc();
            heading.set_text(&format!(
                "{}: {:.0} / {:.0} ({} sessions left)",
                entry.skill, entry.current, entry.cap, entry.sessions_remaining
            ));
            rows.add_child(&heading);
            let mut buy = Button::new_alloc();
            buy.set_text(&format!("Train for {} gold", entry.cost));
            buy.set_disabled(!can_pay);
            buy.connect("pressed", &base.callable("purchase_training_menu"));
            rows.add_child(&buy);
        } else {
            let mut unavailable = Label::new_alloc();
            unavailable.set_text("Training is unavailable.");
            rows.add_child(&unavailable);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Open the trainer menu requested by Game.ShowTrainingMenu.
    pub fn open_training_menu(&mut self, trainer: GlobalFormId) -> bool {
        if self.game.is_none() || !self.open_menu("Training".into()) {
            return false;
        }
        self.training_trainer = Some(trainer);
        if self.training_content() {
            true
        } else {
            self.close_menu();
            false
        }
    }

    /// Complete one training purchase and redraw the current offer.
    #[func]
    pub fn purchase_training_menu(&mut self) -> bool {
        if self.menu.title() != Some("Training") {
            return false;
        }
        let Some(trainer) = self.training_trainer else {
            return false;
        };
        let purchased = self
            .game
            .as_mut()
            .is_some_and(|game| training_menu::purchase(game, trainer).is_ok());
        self.training_content();
        purchased
    }
}

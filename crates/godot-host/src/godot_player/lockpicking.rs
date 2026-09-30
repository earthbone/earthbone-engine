// SPDX-License-Identifier: GPL-3.0-or-later

//! Lockpicking controls route angle attempts through the shared Game.

use godot::classes::{Button, HSlider, Label};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;

impl PlayerNode {
    pub(super) fn open_lockpicking_menu(&mut self, target: GlobalFormId) -> bool {
        if !self.open_menu("Lockpicking".into()) {
            return false;
        }
        self.lockpick_target = Some(target);
        self.lockpick_angle = 0.0;
        let Some(mut content) = self.menu_content() else {
            self.close_menu();
            return false;
        };
        let base = self.base().clone();
        let _guard = self.base_mut();
        let mut description = Label::new_alloc();
        description.set_text("Set the pick angle, then turn the lock.");
        content.add_child(&description);
        let mut slider = HSlider::new_alloc();
        slider.set_min(0.0);
        slider.set_max(90.0);
        slider.set_step(1.0);
        slider.set_value(0.0);
        slider.set_name("PickAngle");
        slider.connect("value_changed", &base.callable("lockpick_set_angle"));
        content.add_child(&slider);
        let mut status = Label::new_alloc();
        status.set_name("LockpickStatus");
        status.set_text("Ready");
        content.add_child(&status);
        let mut turn = Button::new_alloc();
        turn.set_name("TurnLock");
        turn.set_text("Turn lock");
        turn.connect(
            "pressed",
            &base
                .callable("lockpick_turn")
                .bindv(&varray![i64::from(target.file), i64::from(target.object),]),
        );
        content.add_child(&turn);
        drop(_guard);
        self.lockpick_status = Some(status);
        true
    }

    fn try_lockpick(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let target = GlobalFormId { file, object };
        if !self.menu.is_open() || self.menu.title() != Some("Lockpicking") {
            return false;
        }
        if self.lockpick_target != Some(target) {
            return false;
        }
        let Some(pick) = self
            .game
            .as_ref()
            .and_then(|game| game.world().form("Lockpick"))
        else {
            self.set_lockpick_status("No lockpick record is loaded");
            return false;
        };
        let player = self.game.as_mut().map_or(target, |game| game.player());
        let has_pick = self
            .game
            .as_mut()
            .is_some_and(|game| game.world_mut().item_count(player, pick) > 0);
        if !has_pick {
            self.set_lockpick_status("No lockpicks remain");
            return false;
        }
        let opened = self
            .game
            .as_mut()
            .is_some_and(|game| game.attempt_lockpick(target, player, pick, self.lockpick_angle));
        if opened {
            self.close_menu();
            self.activate_game_reference(target);
            true
        } else {
            self.set_lockpick_status("The pick broke. Adjust the angle and try again.");
            false
        }
    }

    fn set_lockpick_status(&mut self, text: &str) {
        if let Some(status) = self.lockpick_status.as_mut() {
            status.set_text(text);
        }
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Set the angle used by the next lockpick turn.
    #[func]
    pub fn lockpick_set_angle(&mut self, angle: f64) -> bool {
        if !self.menu.is_open() || self.menu.title() != Some("Lockpicking") || !angle.is_finite() {
            return false;
        }
        self.lockpick_angle = angle.clamp(0.0, 90.0) as f32;
        true
    }

    /// Turn the lock at the selected pick angle.
    #[func]
    pub fn lockpick_turn(&mut self, file: i64, object: i64) -> bool {
        self.try_lockpick(file, object)
    }
}

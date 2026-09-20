// SPDX-License-Identifier: GPL-3.0-or-later

//! One modal menu over the shared Game. Game::tick_menu_mode owns clock semantics.
//! Godot Control and Theme provide presentation, without a second world model.

use soe_game::Game;

/// Host menu state. All gameplay data remains in the supplied shared Game.
#[derive(Debug, Default)]
pub struct MenuState {
    title: Option<String>,
}

impl MenuState {
    /// Open one menu, rejecting empty titles and a second concurrent menu.
    pub fn open(&mut self, title: &str) -> bool {
        if self.is_open() || title.trim().is_empty() {
            return false;
        }
        self.title = Some(title.into());
        true
    }

    /// Close the current menu. Repeated closes leave the state unchanged.
    pub fn close(&mut self) -> bool {
        self.title.take().is_some()
    }

    pub fn is_open(&self) -> bool {
        self.title.is_some()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Tick the existing Game, preserving its menu and gameplay clock domains.
    pub fn tick(&self, game: &mut Game, seconds: f64) {
        game.tick_menu_mode(seconds, self.is_open());
    }
}

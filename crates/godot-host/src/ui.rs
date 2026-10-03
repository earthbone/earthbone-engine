// SPDX-License-Identifier: GPL-3.0-or-later

//! One modal menu over the shared Game. Game::tick_menu_mode owns clock semantics.
//! Godot Control and Theme provide presentation, without a second world model.

use soe_game::world::HostRequest;
use soe_game::Game;

/// Host menu state. All gameplay data remains in the supplied shared Game.
#[derive(Debug, Default)]
pub struct MenuState {
    title: Option<String>,
}

/// Host screen selected after a game native requests title presentation.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MainMenuState {
    active: bool,
    sequence: Option<String>,
}

impl MainMenuState {
    /// Return to the main menu and clear any active title sequence.
    pub fn enter(&mut self) {
        self.active = true;
        self.sequence = None;
    }

    /// Leave the main menu when a new game runtime starts.
    pub fn leave(&mut self) {
        self.active = false;
        self.sequence = None;
    }

    /// Enter the main menu and start the named title sequence.
    pub fn start_sequence(&mut self, name: String) {
        self.active = true;
        self.sequence = Some(name);
    }

    /// Report whether the host is showing the main menu.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Report whether the main menu is presenting a title sequence.
    pub fn title_sequence(&self) -> Option<&str> {
        self.sequence.as_deref()
    }
}

/// Apply one game request to the host title screen.
pub fn apply_host_request(menu: &mut MainMenuState, request: HostRequest) {
    match request {
        HostRequest::QuitToMainMenu => menu.enter(),
        HostRequest::StartTitleSequence(name) => menu.start_sequence(name),
        HostRequest::FadeOutGame { .. }
        | HostRequest::Save { .. }
        | HostRequest::CameraFirstPerson
        | HostRequest::CameraThirdPerson
        | HostRequest::ControllerShake { .. } => {}
    }
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

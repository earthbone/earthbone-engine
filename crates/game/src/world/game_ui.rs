// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient requests from Papyrus Game functions to the host presenter.

use super::{BinkOptions, GameUiRequest, World};

/// Invalid arguments to a host presentation request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GameUiError {
    /// A Bink request needs a non-empty file name.
    #[error("Bink file name is empty")]
    EmptyVideoName,
}

impl World {
    /// Record an invalid argument from a Papyrus native whose return type is void.
    pub fn report_native_error(&mut self, native: &str, error: String) {
        self.native_errors.push((native.to_owned(), error));
    }

    /// Set the HUD's cart-mode flag.
    pub fn set_hud_cart_mode(&mut self, enabled: bool) {
        self.hud_cart_mode = enabled;
    }

    /// Queue the race and sex editor for the host presenter.
    pub fn show_race_menu(&mut self) {
        self.game_ui_requests.push(GameUiRequest::ShowRaceMenu);
        self.menu_requests.push(super::MenuRequest::Race);
    }

    /// Queue a screen blood effect for the host presenter.
    pub fn trigger_screen_blood(&mut self, value: i32) {
        self.game_ui_requests
            .push(GameUiRequest::TriggerScreenBlood(value));
    }

    /// Queue Bink playback after validating the requested file name.
    ///
    /// # Errors
    /// Returns [`GameUiError::EmptyVideoName`] when `file_name` is empty or
    /// contains only whitespace. No request is queued on error.
    pub fn play_bink(&mut self, file_name: &str, options: BinkOptions) -> Result<(), GameUiError> {
        if file_name.trim().is_empty() {
            return Err(GameUiError::EmptyVideoName);
        }
        self.game_ui_requests.push(GameUiRequest::PlayBink {
            file_name: file_name.to_owned(),
            options,
        });
        Ok(())
    }

    /// Take pending host presentation requests in their original order.
    pub fn take_game_ui_requests(&mut self) -> Vec<GameUiRequest> {
        std::mem::take(&mut self.game_ui_requests)
    }
}

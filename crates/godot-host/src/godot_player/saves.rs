// SPDX-License-Identifier: GPL-3.0-or-later

//! Native generation entry points on the host's shared Game.

use super::PlayerNode;
use godot::classes::ProjectSettings;
use godot::prelude::*;
use soe_game::save_generations::{
    Error, GenerationStore, GenerationWrite, RejectedGeneration, RejectionReason,
};

fn requested_save_uri(automatic: bool) -> String {
    let kind = if automatic { "automatic" } else { "manual" };
    format!("user://saves/{kind}")
}

fn write_generation(
    game: &mut soe_game::Game,
    store: &GenerationStore,
) -> Result<GenerationWrite, Error> {
    game.write_generation(store)
}

fn store(directory: &GString, retained: i64) -> Result<GenerationStore, String> {
    let retained =
        usize::try_from(retained).map_err(|_| "retention must be between 2 and 64".to_string())?;
    let path = ProjectSettings::singleton()
        .globalize_path(directory)
        .to_string();
    GenerationStore::new(path, retained).map_err(|error| error.to_string())
}

fn rejected(candidates: &[RejectedGeneration]) -> Array<VarDictionary> {
    let mut result = Array::new();
    for candidate in candidates {
        let mut entry = VarDictionary::new();
        entry.set("generation", candidate.generation.to_string().as_str());
        entry.set("path", candidate.path.to_string_lossy().as_ref());
        let (kind, detail) = match &candidate.reason {
            RejectionReason::Integrity(detail) => ("integrity", detail),
            RejectionReason::Definitions(detail) => ("definitions", detail),
            RejectionReason::Read(detail) => ("read", detail),
        };
        entry.set("reason", kind);
        entry.set("detail", detail.as_str());
        result.push(&entry);
    }
    result
}

fn failed(message: &str) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("ok", false);
    result.set("state_loaded", false);
    result.set("error", message);
    result
}

fn failed_save(error: &Error) -> VarDictionary {
    let mut result = failed(&error.to_string());
    if let Error::NoLoadable {
        rejected: candidates,
    } = error
    {
        result.set("rejected", &rejected(candidates));
    }
    result
}

#[godot_api(secondary)]
impl PlayerNode {
    pub(super) fn write_requested_save(&mut self, automatic: bool) {
        let kind = if automatic { "automatic" } else { "manual" };
        let directory_path = requested_save_uri(automatic);
        let directory = GString::from(directory_path.as_str());
        let path = ProjectSettings::singleton()
            .globalize_path(&directory)
            .to_string();
        if let Err(error) = std::fs::create_dir_all(&path) {
            soe_trace::error!(target: "host::saves", system = "host::saves", "PlayerNode: could not create {kind} save directory: {error}");
            return;
        }
        let result = self.save_game_generation(directory, 3);
        if !result
            .get("ok")
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false)
        {
            let error = result
                .get("error")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_else(|| "unknown save error".into());
            soe_trace::error!(target: "host::saves", system = "host::saves", "PlayerNode: {kind} save request failed: {error}");
        }
    }

    /// Save the shared runtime in an existing dedicated directory, including the
    /// current controller pose. Returns diagnostics and the committed generation.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn save_game_generation(&mut self, directory: GString, retained: i64) -> VarDictionary {
        let store = match store(&directory, retained) {
            Ok(store) => store,
            Err(error) => return failed(&error),
        };
        let feet = self.feet();
        let rotation = [-self.rig.pitch(), 0.0, self.rig.yaw()];
        let Some(game) = &mut self.game else {
            return failed("gameplay has not started");
        };
        let player_id = game.player();
        if let Some(player) = game.world_mut().reference(player_id) {
            player.position = [feet.x, feet.y, feet.z];
            player.rotation = rotation;
        }
        match write_generation(game, &store) {
            Ok(report) => {
                let mut result = VarDictionary::new();
                result.set("ok", true);
                result.set("generation", report.generation.to_string().as_str());
                result.set("path", report.path.to_string_lossy().as_ref());
                result.set("retained", report.retained.len() as i64);
                result.set("rejected", &rejected(&report.rejected));
                result
            }
            Err(error) => failed_save(&error),
        }
    }

    /// Explicitly recover the newest verified native generation. Rejected files
    /// leave the game unchanged. Resource installation failure stops stepping.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn recover_game_generation(&mut self, directory: GString, retained: i64) -> VarDictionary {
        let store = match store(&directory, retained) {
            Ok(store) => store,
            Err(error) => return failed(&error),
        };
        let Some(game) = &mut self.game else {
            return failed("gameplay has not started");
        };
        let selected = match game.recover_generation(&store) {
            Ok(selected) => selected,
            Err(error) => return failed_save(&error),
        };
        self.melee_contact.cancel();
        self.melee_bodies.clear();
        self.last_activate_reference = None;
        self.footstep_cadence.reset();
        let installed = self.sync_game_pose();
        if !installed {
            self.auto_step = false;
        }
        let mut result = VarDictionary::new();
        result.set("ok", installed);
        result.set("state_loaded", true);
        result.set("generation", selected.generation.to_string().as_str());
        result.set("path", selected.path.to_string_lossy().as_ref());
        result.set("rejected", &rejected(&selected.rejected));
        if !installed {
            result.set(
                "error",
                "save loaded but host resources could not be installed, stepping stopped",
            );
        }
        result
    }

    /// Atomically load a verified native generation into a fresh Game, then
    /// install its player cell and camera before replacing the active Game.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn attach_restored_generation(
        &mut self,
        directory: GString,
        retained: i64,
    ) -> VarDictionary {
        let store = match store(&directory, retained) {
            Ok(store) => store,
            Err(error) => return failed(&error),
        };
        let Some(source) = &self.data_source else {
            return failed("game data has not been loaded");
        };
        if self.game_streamer.is_none() {
            return failed("cell streamer data has not been shared");
        }

        let mut game = match source.localized_game() {
            Ok(game) => game,
            Err(error) => return failed(&error.to_string()),
        };
        let scripts = game.load_scripts_from_vfs(source.vfs());
        if !scripts.scripts_failed.is_empty() {
            return failed(&format!(
                "could not load {} saved Papyrus definitions",
                scripts.scripts_failed.len()
            ));
        }
        let selected = match game.recover_generation(&store) {
            Ok(selected) => selected,
            Err(error) => return failed_save(&error),
        };
        if !self.attach_restored_game(game) {
            return failed("restored game host resources could not be installed");
        }

        let mut result = VarDictionary::new();
        result.set("ok", true);
        result.set("state_loaded", true);
        result.set("generation", selected.generation.to_string().as_str());
        result.set("path", selected.path.to_string_lossy().as_ref());
        result.set("rejected", &rejected(&selected.rejected));
        result
    }
}

#[cfg(test)]
mod scr_28_tests {
    use super::*;
    use soe_esm::LoadOrder;

    #[test]
    fn scr_28_save_requests_write_generations_to_distinct_manual_and_auto_stores() {
        let directory = tempfile::tempdir().unwrap();
        let manual_path = directory.path().join("manual");
        let automatic_path = directory.path().join("automatic");
        let manual_uri = requested_save_uri(false);
        let automatic_uri = requested_save_uri(true);
        assert_eq!(manual_uri, "user://saves/manual");
        assert_eq!(automatic_uri, "user://saves/automatic");
        std::fs::create_dir_all(&manual_path).unwrap();
        std::fs::create_dir_all(&automatic_path).unwrap();

        let mut game = soe_game::Game::new(LoadOrder::new());
        let manual =
            write_generation(&mut game, &GenerationStore::new(&manual_path, 3).unwrap()).unwrap();
        let automatic = write_generation(
            &mut game,
            &GenerationStore::new(&automatic_path, 3).unwrap(),
        )
        .unwrap();
        assert_eq!(manual.generation, 1);
        assert_eq!(automatic.generation, 1);
        assert!(manual.path.is_file());
        assert!(automatic.path.is_file());
        assert_ne!(manual.path.parent(), automatic.path.parent());
    }
}

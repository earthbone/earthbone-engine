// SPDX-License-Identifier: GPL-3.0-or-later

//! Mod Organizer 2 instances and profiles, read from disk.
//!
//! MO2 has no published format. What follows is the layout its `profile.cpp`
//! writes and every other tool reads: a `mods/` folder with one directory per
//! mod, and per profile `modlist.txt` (one mod per line, highest priority
//! first, `+` enabled, `-` disabled, `*` for unmanaged content such as DLC),
//! `plugins.txt` and `loadorder.txt` in the game's own formats, `archives.txt`
//! listing the archives checked in the Archives tab, and `settings.ini` with
//! `LocalSaves`, `LocalSettings` and `AutomaticArchiveInvalidation`. With
//! `LocalSettings` on, the profile folder holds its own `Skyrim.ini`,
//! `SkyrimPrefs.ini` and `SkyrimCustom.ini`, otherwise the game's are used.
//! `overwrite/` in the instance root sits above every mod.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::ini::Ini;
use crate::plugins_txt::{read_loadorder_txt, read_plugins_txt, PluginLine};
use crate::{LoadoutInput, INI_NAMES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModState {
    Enabled,
    Disabled,
    /// A `*` line: DLC or other content MO2 shows but does not manage.
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModLine {
    pub name: String,
    pub state: ModState,
}

/// File order, which is highest priority first.
pub fn parse_modlist(text: &str) -> Vec<ModLine> {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (state, name) = match l.chars().next()? {
                '+' => (ModState::Enabled, &l[1..]),
                '-' => (ModState::Disabled, &l[1..]),
                '*' => (ModState::Unmanaged, &l[1..]),
                _ => return None,
            };
            Some(ModLine {
                name: name.trim().to_string(),
                state,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSettings {
    pub local_saves: bool,
    pub local_settings: bool,
    pub automatic_archive_invalidation: bool,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        ProfileSettings {
            local_saves: false,
            local_settings: false,
            automatic_archive_invalidation: true,
        }
    }
}

impl ProfileSettings {
    pub fn parse(text: &str) -> ProfileSettings {
        let ini = Ini::parse(text);
        let flag = |key: &str, default: bool| {
            ini.get("General", key)
                .map(|v| matches!(v.trim(), "true" | "1" | "True"))
                .unwrap_or(default)
        };
        ProfileSettings {
            local_saves: flag("LocalSaves", false),
            local_settings: flag("LocalSettings", false),
            automatic_archive_invalidation: flag("AutomaticArchiveInvalidation", true),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Mo2Profile {
    pub dir: PathBuf,
    /// Lowest priority first, so a later mod wins, the way data roots read.
    pub mods_low_to_high: Vec<ModLine>,
    pub plugins: Vec<PluginLine>,
    pub loadorder: Vec<String>,
    pub archives: Vec<String>,
    pub settings: ProfileSettings,
}

fn read_or_empty(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

impl Mo2Profile {
    pub fn read(dir: &Path) -> Result<Mo2Profile> {
        let mut mods = parse_modlist(&read_or_empty(&dir.join("modlist.txt"))?);
        mods.reverse();
        let archives = read_or_empty(&dir.join("archives.txt"))?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        Ok(Mo2Profile {
            dir: dir.to_path_buf(),
            mods_low_to_high: mods,
            plugins: read_plugins_txt(&dir.join("plugins.txt"))?,
            loadorder: read_loadorder_txt(&dir.join("loadorder.txt"))?,
            archives,
            settings: ProfileSettings::parse(&read_or_empty(&dir.join("settings.ini"))?),
        })
    }

    pub fn enabled_mods(&self) -> impl Iterator<Item = &str> {
        self.mods_low_to_high
            .iter()
            .filter(|m| m.state == ModState::Enabled)
            .map(|m| m.name.as_str())
    }

    /// Where the game INIs for this profile live: the profile itself with
    /// local settings, the game's own folder otherwise.
    pub fn ini_dir(&self, my_games: Option<&Path>) -> Option<PathBuf> {
        if self.settings.local_settings {
            Some(self.dir.clone())
        } else {
            my_games.map(Path::to_path_buf)
        }
    }

    pub fn saves_dir(&self, my_games: Option<&Path>) -> Option<PathBuf> {
        if self.settings.local_saves {
            Some(self.dir.join("saves"))
        } else {
            my_games.map(|p| p.join("Saves"))
        }
    }
}

/// One MO2 instance: where the game is, where the mods are, which profile.
#[derive(Debug, Clone)]
pub struct Mo2Instance {
    pub game_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub profile_dir: PathBuf,
    /// `Documents/My Games/Skyrim Special Edition`, when the profile does not
    /// keep its own INIs.
    pub my_games: Option<PathBuf>,
    /// The instance's `overwrite/`, above every mod when present.
    pub overwrite_dir: Option<PathBuf>,
}

impl Mo2Instance {
    /// A portable instance: `mods/`, `profiles/<name>/` and `overwrite/`
    /// under one root.
    pub fn portable(root: &Path, game_dir: &Path, profile: &str) -> Mo2Instance {
        Mo2Instance {
            game_dir: game_dir.to_path_buf(),
            mods_dir: root.join("mods"),
            profile_dir: root.join("profiles").join(profile),
            my_games: None,
            overwrite_dir: Some(root.join("overwrite")),
        }
    }

    pub fn profile(&self) -> Result<Mo2Profile> {
        Mo2Profile::read(&self.profile_dir)
    }

    /// The engine's inputs for this profile: the game's Data, then each
    /// enabled mod low to high priority, then overwrite, with the profile's
    /// lists and INIs.
    pub fn loadout_input(&self) -> Result<LoadoutInput> {
        let profile = self.profile()?;
        let mut data_roots = vec![self.game_dir.join("Data")];
        for name in profile.enabled_mods() {
            let dir = self.mods_dir.join(name);
            if dir.is_dir() {
                data_roots.push(dir);
            }
        }
        if let Some(over) = &self.overwrite_dir {
            if over.is_dir() {
                data_roots.push(over.clone());
            }
        }
        let ini_dir = profile.ini_dir(self.my_games.as_deref());
        Ok(LoadoutInput {
            game_dir: self.game_dir.clone(),
            data_roots,
            plugins_txt: Some(self.profile_dir.join("plugins.txt")),
            loadorder_txt: Some(self.profile_dir.join("loadorder.txt")),
            ini_dir,
            ccc: None,
        })
    }
}

/// The INI file names a profile may carry locally.
pub fn local_ini_names() -> &'static [&'static str] {
    INI_NAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modlist_is_reversed_into_priority_order() {
        let text = "# generated by MO2\r\n+Top.esp mod\r\n-Off\r\n*DLC: Dawnguard\r\n+Bottom\r\n";
        let lines = parse_modlist(text);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].state, ModState::Enabled);
        assert_eq!(lines[1].state, ModState::Disabled);
        assert_eq!(lines[2].state, ModState::Unmanaged);
        assert_eq!(lines[2].name, "DLC: Dawnguard");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("modlist.txt"), text).unwrap();
        std::fs::write(dir.path().join("settings.ini"), "[General]\nLocalSaves=true\nLocalSettings=false\n").unwrap();
        let p = Mo2Profile::read(dir.path()).unwrap();
        let enabled: Vec<&str> = p.enabled_mods().collect();
        assert_eq!(enabled, vec!["Bottom", "Top.esp mod"]);
        assert!(p.settings.local_saves);
        assert!(!p.settings.local_settings);
        assert!(p.settings.automatic_archive_invalidation);
        assert_eq!(p.ini_dir(Some(Path::new("/mg"))), Some(PathBuf::from("/mg")));
        assert_eq!(p.saves_dir(None), Some(dir.path().join("saves")));
    }

    #[test]
    fn local_settings_point_the_inis_at_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.ini"), "[General]\nLocalSettings=true\n").unwrap();
        let p = Mo2Profile::read(dir.path()).unwrap();
        assert_eq!(p.ini_dir(Some(Path::new("/mg"))), Some(dir.path().to_path_buf()));
        assert!(p.mods_low_to_high.is_empty());
        assert!(p.plugins.is_empty());
    }
}

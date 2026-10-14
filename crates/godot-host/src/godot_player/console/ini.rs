// SPDX-License-Identifier: GPL-3.0-or-later

//! UESP REFINI and SAVEINI use the active profile and existing INI layering rules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use soe_profile::{Ini, Layer, Settings};

use super::PlayerNode;

fn runtime_settings(settings: &Settings) -> BTreeMap<String, String> {
    settings
        .iter()
        .map(|((section, key), value)| (format!("{key}:{section}"), value.value.clone()))
        .collect()
}

fn read_settings(
    game_dir: &Path,
    roots: &[PathBuf],
    directory: Option<&Path>,
    plugins: &[String],
) -> Result<Settings, String> {
    let mut settings = Settings::new();
    let mut apply = |path: &Path, layer: Layer| -> Result<(), String> {
        match std::fs::read_to_string(path) {
            Ok(text) => settings.apply(&layer, &Ini::parse(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
        Ok(())
    };
    apply(&game_dir.join("Skyrim_Default.ini"), Layer::Default)?;
    if let Some(directory) = directory {
        for (name, layer) in
            soe_profile::INI_NAMES
                .iter()
                .zip([Layer::Base, Layer::Custom, Layer::Prefs])
        {
            apply(&directory.join(name), layer)?;
        }
    }
    let paths = ini_paths(roots)?;
    for plugin in plugins {
        let stem = Path::new(plugin)
            .file_stem()
            .ok_or("plugin has no stem")?
            .to_string_lossy();
        if let Some(path) = paths.get(&format!("{stem}.ini").to_ascii_lowercase()) {
            apply(path, Layer::Plugin(plugin.clone()))?;
        }
    }
    Ok(settings)
}

fn ini_paths(roots: &[PathBuf]) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut paths = BTreeMap::new();
    for root in roots {
        let mut entries = std::fs::read_dir(root)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            paths.insert(
                entry.file_name().to_string_lossy().to_ascii_lowercase(),
                entry.path(),
            );
        }
    }
    Ok(paths)
}

fn save_settings(
    root: &Path,
    plugin: &str,
    settings: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    if Path::new(plugin)
        .file_name()
        .is_none_or(|name| name != plugin)
    {
        return Err("plugin name must be a filename".into());
    }
    let stem = Path::new(plugin)
        .file_stem()
        .ok_or("plugin has no stem")?
        .to_string_lossy();
    let filename = format!("{stem}.ini");
    let path = ini_paths(&[root.to_path_buf()])?
        .get(&filename.to_ascii_lowercase())
        .cloned()
        .unwrap_or_else(|| root.join(filename));
    let mut sections = BTreeMap::<String, Vec<(String, String)>>::new();
    for (name, value) in settings {
        let (key, section) = name
            .split_once(':')
            .ok_or("INI keys require setting:category")?;
        if key.is_empty()
            || section.is_empty()
            || key.contains(['\n', '\r', '='])
            || section.contains(['\n', '\r', '[', ']'])
            || value.contains(['\n', '\r'])
        {
            return Err("INI key or value contains invalid characters".into());
        }
        sections
            .entry(section.to_owned())
            .or_default()
            .push((key.to_owned(), value.clone()));
    }
    let ini = Ini {
        sections: sections
            .into_iter()
            .map(|(name, entries)| soe_profile::ini::Section { name, entries })
            .collect(),
    };
    std::fs::write(&path, ini.render()).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

impl PlayerNode {
    pub(in crate::godot_player) fn seed_console_ini(
        &self,
        source: &crate::data_source::DataSource,
    ) {
        if let Some(game) = &self.game {
            game.world_mut().ini = runtime_settings(source.profile_papyrus_logs().0)
                .into_iter()
                .collect();
        }
    }

    pub(super) fn console_ini(&mut self, reload: bool) -> Result<(), String> {
        let source = self.data_source.as_ref().ok_or("missing profile")?;
        let plugins: Vec<_> = source
            .order()
            .files()
            .iter()
            .map(|file| file.name.clone())
            .collect();
        let game = self.game.as_ref().ok_or("missing gameplay runtime")?;
        if reload {
            let settings = read_settings(
                source
                    .data_roots()
                    .first()
                    .and_then(|root| root.parent())
                    .ok_or("profile has no game directory")?,
                source.data_roots(),
                source.profile_papyrus_logs().1,
                &plugins,
            )?;
            self.rig
                .settings
                .apply_ini(|section, key| settings.get_float(section, key));
            game.world_mut().ini = runtime_settings(&settings).into_iter().collect();
            self.update_camera();
        } else {
            let plugin = plugins.last().ok_or("no active plugin")?;
            let root = source.data_roots().last().ok_or("missing Data directory")?;
            let current = game
                .world()
                .ini
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            save_settings(root, plugin, &current)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_observes_file_changes_and_save_targets_last_plugin() {
        let directory = tempfile::tempdir().unwrap();
        let game_dir = directory.path().join("Game");
        let root = directory.path().join("ProfileData");
        let profile = directory.path().join("profile");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&game_dir).unwrap();
        std::fs::create_dir(&profile).unwrap();
        std::fs::write(
            game_dir.join("Skyrim_Default.ini"),
            "[Camera]\nfDefaultWorldFOV=65\n",
        )
        .unwrap();
        std::fs::write(
            profile.join("Skyrim.ini"),
            "[Camera]\nfDefaultWorldFOV=80\n",
        )
        .unwrap();
        let plugins = vec!["Skyrim.esm".into(), "Last.esp".into()];
        let roots = vec![root.clone()];
        let settings = read_settings(&game_dir, &roots, Some(&profile), &plugins).unwrap();
        assert_eq!(settings.get_float("Camera", "fDefaultWorldFOV"), Some(80.0));
        let mut current = runtime_settings(&settings);
        current.insert("fdefaultworldfov:camera".into(), "100".into());
        std::fs::write(root.join("last.ini"), "[Camera]\nfDefaultWorldFOV=88\n").unwrap();
        let path = save_settings(&root, plugins.last().unwrap(), &current).unwrap();
        assert_eq!(path, root.join("last.ini"));
        assert!(!root.join("Last.ini").exists());
        let settings = read_settings(&game_dir, &roots, Some(&profile), &plugins).unwrap();
        assert_eq!(
            settings.get_float("Camera", "fDefaultWorldFOV"),
            Some(100.0)
        );
        std::fs::remove_file(path).unwrap();
        std::fs::write(
            profile.join("Skyrim.ini"),
            "[Camera]\nfDefaultWorldFOV=90\n",
        )
        .unwrap();
        assert_eq!(
            read_settings(&game_dir, &roots, Some(&profile), &plugins)
                .unwrap()
                .get_float("Camera", "fDefaultWorldFOV"),
            Some(90.0)
        );
    }
}

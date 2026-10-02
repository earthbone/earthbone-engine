// SPDX-License-Identifier: GPL-3.0-or-later

//! Files retained for the race menu's selectable head parts, using xEdit's
//! HDPT layout and UESP's HDPT NAM0/NAM1 path description.

use std::collections::BTreeMap;

use soe_game::Game;

use super::{DataSource, DataSourceError};
use crate::race_menu;

#[derive(Debug, Default)]
pub(crate) struct ChargenCache {
    files: BTreeMap<String, Vec<u8>>,
    bytes: usize,
    models: usize,
    morphs: usize,
    missing: usize,
}

impl ChargenCache {
    /// Read available model and chargen TRI files for race menu head parts.
    ///
    /// # Errors
    ///
    /// Returns an error when an available archive asset cannot be read.
    /// Paths absent from the active VFS are counted and skipped.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub(crate) fn load(source: &DataSource, game: &Game) -> Result<Self, DataSourceError> {
        let mut paths = BTreeMap::new();
        let world = game.world();
        for race in race_menu::entries(game) {
            for female in [false, true] {
                for part in race_menu::head_parts(game, race.race, female) {
                    let Some(record) = world.order().winner(part.form) else {
                        continue;
                    };
                    if let Some(model) = record
                        .find(soe_esm::Tag(*b"MODL"))
                        .map(soe_esm::cursor::zstring)
                        .filter(|path| !path.is_empty())
                    {
                        let path = mesh_data_path(&model);
                        paths.entry(path.to_ascii_lowercase()).or_insert(path);
                    }
                    let types = record.all(soe_esm::Tag(*b"NAM0"));
                    let names = record.all(soe_esm::Tag(*b"NAM1"));
                    for (kind, name) in types.zip(names) {
                        if kind == [2, 0, 0, 0] {
                            let path = soe_esm::cursor::zstring(name);
                            if !path.is_empty() {
                                let path = mesh_data_path(&path);
                                paths.entry(path.to_ascii_lowercase()).or_insert(path);
                            }
                        }
                    }
                }
            }
        }
        let mut files = BTreeMap::new();
        let mut bytes = 0usize;
        let mut models = 0;
        let mut morphs = 0;
        let mut missing = 0;
        for (key, path) in paths {
            let data = match source.vfs().read(&path) {
                Ok(data) => data,
                Err(soe_bsa::Error::NotFound { .. }) => {
                    missing += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            bytes = bytes.saturating_add(data.len());
            if path.to_ascii_lowercase().ends_with(".tri") {
                morphs += 1;
            } else if path.to_ascii_lowercase().ends_with(".nif") {
                models += 1;
            }
            files.insert(key, data);
        }
        Ok(Self {
            files,
            bytes,
            models,
            morphs,
            missing,
        })
    }

    /// Number of available asset files retained in memory.
    pub(crate) fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Sum of the retained asset file lengths.
    pub(crate) fn resident_bytes(&self) -> usize {
        self.bytes
    }

    /// Number of retained NIF model files.
    pub(crate) fn model_count(&self) -> usize {
        self.models
    }

    /// Number of retained chargen TRI files.
    pub(crate) fn morph_count(&self) -> usize {
        self.morphs
    }

    /// Number of distinct asset paths absent from the active VFS.
    pub(crate) fn missing_count(&self) -> usize {
        self.missing
    }

    /// Drop every retained asset buffer and reset its counts.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// HDPT MODL and NAM1 paths are relative to Data/Meshes in the record format.
fn mesh_data_path(path: &str) -> String {
    if path
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Meshes\\"))
    {
        path.to_string()
    } else {
        format!("Meshes\\{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_17_chargen_cache_counts_unique_retained_assets_and_bytes() {
        let cache = ChargenCache {
            files: BTreeMap::from([
                ("meshes/part.nif".into(), vec![1; 7]),
                ("meshes/morph.tri".into(), vec![2; 5]),
            ]),
            bytes: 12,
            models: 1,
            morphs: 1,
            missing: 0,
        };
        assert_eq!(cache.file_count(), 2);
        assert_eq!(cache.resident_bytes(), 12);
    }

    #[test]
    fn ui_17_chargen_cache_clear_releases_files_and_resets_accounting() {
        let mut cache = ChargenCache {
            files: BTreeMap::from([("meshes/part.nif".into(), vec![1; 7])]),
            bytes: 7,
            models: 1,
            morphs: 0,
            missing: 0,
        };
        cache.clear();
        assert_eq!(cache.file_count(), 0);
        assert_eq!(cache.resident_bytes(), 0);
        assert_eq!(cache.model_count(), 0);
        assert_eq!(cache.morph_count(), 0);
        assert_eq!(cache.missing_count(), 0);
    }

    #[test]
    fn ui_17_chargen_asset_paths_resolve_from_mesh_root() {
        assert_eq!(
            mesh_data_path("Actors\\Character\\head.nif"),
            "Meshes\\Actors\\Character\\head.nif"
        );
        assert_eq!(
            mesh_data_path("Meshes\\Actors\\Character\\head.nif"),
            "Meshes\\Actors\\Character\\head.nif"
        );
    }

    #[test]
    fn ui_17_chargen_cache_loads_vanilla_head_part_and_morph_assets() {
        let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
            return;
        };
        let source = DataSource::open(std::path::Path::new(&data_dir)).unwrap();
        let game = Game::from_shared_order(source.shared_order());
        let cache = ChargenCache::load(&source, &game).unwrap();
        assert!(
            cache.model_count() > 0,
            "no resolved model files, missing={}, retained={}",
            cache.missing_count(),
            cache.file_count()
        );
        assert!(
            cache.morph_count() > 0,
            "no resolved morph files, missing={}, retained={}",
            cache.missing_count(),
            cache.file_count()
        );
        assert!(cache.resident_bytes() > 0);
        assert_eq!(
            cache.resident_bytes(),
            cache.files.values().map(Vec::len).sum::<usize>()
        );
    }
}

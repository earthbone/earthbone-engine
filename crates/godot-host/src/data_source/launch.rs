// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit Data roots use soe-profile's master ordering and bsa's loose
//! precedence. See docs/profile.md and LMO src/plugins/loadorder.rs.

use super::{DataSource, DataSourceError};
use std::path::{Path, PathBuf};

pub(super) fn loadout(
    data: &Path,
    overlays: &[PathBuf],
) -> Result<soe_profile::Loadout, DataSourceError> {
    let game = data
        .parent()
        .ok_or_else(|| DataSourceError::DataDirectory(data.to_path_buf()))?;
    let mut roots = vec![data.to_path_buf()];
    for root in overlays {
        if !root.is_dir() {
            return Err(DataSourceError::DataDirectory(root.clone()));
        }
        roots.push(root.clone());
    }
    let mut loadout = soe_profile::LoadoutInput {
        game_dir: game.to_path_buf(),
        data_roots: roots,
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    }
    .resolve()?;
    if loadout.load_order.broken()
        || loadout
            .load_order
            .problems
            .iter()
            .any(|problem| matches!(problem, soe_profile::Problem::Unreadable { .. }))
    {
        return Err(DataSourceError::LoadOrder(format!(
            "{:?}",
            loadout.load_order.problems
        )));
    }
    if overlays.is_empty() {
        return Ok(loadout);
    }
    let headers = loadout
        .load_order
        .entries
        .iter()
        .map(|entry| soe_profile::PluginHeader::read(&entry.path))
        .collect::<Result<Vec<_>, _>>()?;
    let early = loadout
        .load_order
        .entries
        .iter()
        .filter(|entry| entry.early)
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
    let mut plugins = Vec::new();
    for root in overlays {
        let mut entries = loadout
            .load_order
            .entries
            .iter()
            .filter(|entry| entry.path.parent() == Some(root.as_path()))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.name.to_ascii_lowercase());
        plugins.extend(entries.into_iter().map(|entry| soe_profile::PluginLine {
            name: entry.name.clone(),
            enabled: true,
        }));
    }
    loadout.load_order = soe_profile::order::resolve(&soe_profile::order::Inputs {
        available: &headers,
        plugins_txt: &plugins,
        loadorder_txt: &[],
        early: &early,
    })?;
    for root in overlays {
        let mut archives = std::fs::read_dir(root)
            .map_err(|source| DataSourceError::Io {
                path: root.clone(),
                source,
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| DataSourceError::Io {
                path: root.clone(),
                source,
            })?;
        archives.retain(|path| {
            path.is_file()
                && path.extension().is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("bsa") || extension.eq_ignore_ascii_case("ba2")
                })
        });
        archives.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
        for path in archives {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            loadout
                .archives
                .retain(|archive| !archive.name.eq_ignore_ascii_case(&name));
            loadout.archives.push(soe_profile::ArchiveRef {
                name,
                path: Some(path),
                owner: None,
            });
        }
    }
    Ok(loadout)
}

impl DataSource {
    /// Open the player's base Data and ordered SOE_DATA_PATHS overlays.
    /// Paths use the operating system's search-path separator.
    pub fn open_for_play(data: &Path) -> Result<Self, DataSourceError> {
        let overlays = std::env::var_os("SOE_DATA_PATHS")
            .filter(|paths| !paths.is_empty())
            .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .unwrap_or_default();
        Self::open_loadout(&loadout(data, &overlays)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, GlobalFormId, Plugin, Record, Tag};

    fn base(root: &Path) -> PathBuf {
        let data = root.join("Data");
        std::fs::create_dir(&data).unwrap();
        for name in soe_profile::ccc::HARDCODED_SE {
            let mut plugin = Plugin::new(FileHeader::default());
            if *name == "Skyrim.esm" {
                plugin.push_top(Record::new(Tag(*b"MISC"), 0x800).sub(Tag(*b"FULL"), b"Base\0"));
            }
            std::fs::write(data.join(name), plugin.to_bytes()).unwrap();
        }
        data
    }

    #[test]
    fn overlay_plugins_override_records_and_later_loose_roots_win() {
        let root = tempfile::tempdir().unwrap();
        let data = base(root.path());
        let first = root.path().join("First");
        let last = root.path().join("Last");
        for (path, name) in [(&first, "First.esp"), (&last, "Last.esp")] {
            std::fs::create_dir(path).unwrap();
            let mut plugin = Plugin::new(FileHeader {
                masters: vec!["Skyrim.esm".into()],
                ..FileHeader::default()
            });
            plugin.push_top(Record::new(Tag(*b"MISC"), 0x800).sub(Tag(*b"FULL"), name.as_bytes()));
            std::fs::write(path.join(name), plugin.to_bytes()).unwrap();
            std::fs::create_dir(path.join("Interface")).unwrap();
            std::fs::write(path.join("Interface/inventorymenu.swf"), name).unwrap();
        }
        let profile = loadout(&data, &[first, last]).unwrap();
        let source = DataSource::open_loadout_with_threads_and_cache(&profile, 1, None).unwrap();
        let record = source
            .order()
            .winner(GlobalFormId {
                file: 0,
                object: 0x800,
            })
            .unwrap();
        assert_eq!(record.find(Tag(*b"FULL")), Some(b"Last.esp".as_slice()));
        assert_eq!(
            source.vfs().read("interface/INVENTORYMENU.swf").unwrap(),
            b"Last.esp"
        );
        assert_eq!(source.order().files().last().unwrap().name, "Last.esp");
    }

    #[test]
    fn overlay_esl_extension_uses_the_profiles_light_slot() {
        let root = tempfile::tempdir().unwrap();
        let data = base(root.path());
        let extra = root.path().join("Light");
        std::fs::create_dir(&extra).unwrap();
        let plugin = Plugin::new(FileHeader::default());
        std::fs::write(extra.join("Light.esl"), plugin.to_bytes()).unwrap();
        let profile = loadout(&data, &[extra]).unwrap();
        let source = DataSource::open_loadout_with_threads_and_cache(&profile, 1, None).unwrap();
        assert_eq!(
            source.order().files().last().unwrap().slot,
            soe_esm::Slot::Light(0)
        );
    }

    #[test]
    fn overlay_archives_enter_the_resolved_view_and_bad_roots_fail() {
        let root = tempfile::tempdir().unwrap();
        let data = base(root.path());
        let extra = root.path().join("Extra");
        std::fs::create_dir(&extra).unwrap();
        let archive = soe_bsa::write_bsa(
            &[soe_bsa::BsaFile {
                path: "Interface/inventorymenu.swf",
                bytes: b"Archived theme",
            }],
            soe_bsa::BsaVersion::SkyrimSe,
        )
        .unwrap();
        std::fs::write(extra.join("Theme.BSA"), archive).unwrap();
        let profile = loadout(&data, std::slice::from_ref(&extra)).unwrap();
        assert_eq!(
            profile.archives.last().unwrap().path,
            Some(extra.join("Theme.BSA"))
        );
        let source = DataSource::open_loadout_with_threads_and_cache(&profile, 1, None).unwrap();
        assert_eq!(
            source.vfs().read("interface/inventorymenu.swf").unwrap(),
            b"Archived theme"
        );
        std::fs::write(extra.join("Theme.BSA"), []).unwrap();
        assert!(DataSource::open_loadout_with_threads_and_cache(&profile, 1, None).is_err());
        assert!(matches!(
            loadout(&data, &[extra.join("Missing")]),
            Err(DataSourceError::DataDirectory(_))
        ));
    }
}

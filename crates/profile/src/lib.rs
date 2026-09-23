// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine should read for one loadout: which Data folders, which
//! plugins in which order and slots, which archives, and the merged INI
//! settings. The conventions come from the game (`plugins.txt`, the INI
//! layering, `Skyrim.ccc`) and from Mod Organizer 2 (profiles over a mods
//! folder), so a manager profile is playable without a virtual file system
//! in between.
//!
//! ```no_run
//! use std::path::Path;
//! use soe_profile::mo2::Mo2Instance;
//!
//! let instance = Mo2Instance::portable(Path::new("/mo2"), Path::new("/games/skyrim"), "Default");
//! let loadout = instance.loadout_input().unwrap().resolve().unwrap();
//! for e in loadout.load_order.active() {
//!     println!("{:?} {}", e.slot, e.name);
//! }
//! println!("archives: {:?}", loadout.archives);
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod ccc;
pub mod error;
pub mod header;
pub mod ini;
pub mod mo2;
pub mod order;
pub mod plugins_txt;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use error::{Error, Result};
pub use header::PluginHeader;
pub use ini::{Ini, Layer, Settings};
pub use order::{Entry, LoadOrder, Problem, Slot};
pub use plugins_txt::PluginLine;

/// The game INIs in the order they layer, lowest first.
pub const INI_NAMES: &[&str] = &["Skyrim.ini", "SkyrimCustom.ini", "SkyrimPrefs.ini"];

/// An archive the engine will open, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveRef {
    pub name: String,
    /// Where it was found across the data roots, `None` when a list names
    /// it and no root has it.
    pub path: Option<PathBuf>,
    /// The plugin that pulled it in, `None` for the INI lists.
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Loadout {
    /// Lowest first, a later root shadows an earlier one file by file.
    pub data_roots: Vec<PathBuf>,
    pub load_order: LoadOrder,
    pub settings: Settings,
    /// The INI lists first, then each active plugin's own archives in load
    /// order, the engine's opening order.
    pub archives: Vec<ArchiveRef>,
}

impl Loadout {
    /// Where a Data-relative path resolves, highest root first.
    pub fn find(&self, relative: &str) -> Option<PathBuf> {
        let rel = Path::new(relative);
        self.data_roots
            .iter()
            .rev()
            .map(|r| r.join(rel))
            .find(|p| p.exists())
    }

    /// Write this loadout to `dest` as a standalone install the engine reads
    /// without a mod manager in the loop: `Data` merges every root in order,
    /// a later one shadowing an earlier one file by file, the same rule
    /// `find` uses. `plugins.txt` and `loadorder.txt` carry the resolved
    /// order, `Skyrim.ccc` the Creation Club names this loadout treated as
    /// early loaders, and `Skyrim.ini`, `SkyrimCustom.ini`, `SkyrimPrefs.ini`
    /// the settings layered above the game's own defaults.
    ///
    /// A plugin's own INI travels with it, since it already sits next to the
    /// plugin in a data root and is copied along with everything else there.
    /// `Skyrim_Default.ini` and whatever it sets are not written here, that
    /// file belongs to the game install the package lands in, not to the
    /// profile. Reload the result with a `LoadoutInput` whose `data_roots` is
    /// `[dest.join("Data")]`, `plugins_txt` and `loadorder_txt` point at the
    /// files here, `ini_dir` is `dest`, and `game_dir` and `ccc` point at
    /// wherever `Skyrim_Default.ini` and the game's own `Skyrim.ccc` live
    /// (`dest` itself, for a fully standalone copy).
    pub fn package(&self, dest: &Path) -> Result<()> {
        let data_dir = dest.join("Data");
        for root in &self.data_roots {
            copy_tree(root, &data_dir)?;
        }

        write_text(
            &dest.join("plugins.txt"),
            &plugins_txt::render_plugins_txt(&self.load_order.plugins_txt_lines()),
        )?;
        write_text(
            &dest.join("loadorder.txt"),
            &plugins_txt::render_loadorder_txt(&self.load_order.loadorder_txt_names()),
        )?;

        let hardcoded: std::collections::HashSet<String> = ccc::HARDCODED_SE
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let creations: Vec<&str> = self
            .load_order
            .entries
            .iter()
            .filter(|e| e.early && !hardcoded.contains(&e.name.to_ascii_lowercase()))
            .map(|e| e.name.as_str())
            .collect();
        if !creations.is_empty() {
            let mut text = String::new();
            for name in &creations {
                text.push_str(name);
                text.push_str("\r\n");
            }
            write_text(&dest.join("Skyrim.ccc"), &text)?;
        }

        for (name, layer) in INI_NAMES
            .iter()
            .zip([Layer::Base, Layer::Custom, Layer::Prefs])
        {
            let ini = settings_for_layer(&self.settings, &layer);
            if !ini.sections.is_empty() {
                write_text(&dest.join(name), &ini.render())?;
            }
        }

        Ok(())
    }
}

/// The values one layer carries, grouped back into sections for `Ini::render`.
/// Section and key names come back lowercased, `Settings` keeps only the
/// normalized form, the game reads both case-insensitively.
fn settings_for_layer(settings: &Settings, layer: &Layer) -> Ini {
    let mut sections: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    for (id, value) in settings.iter() {
        if value.layer == *layer {
            let (section, key) = id;
            sections
                .entry(section.clone())
                .or_default()
                .push((key.clone(), value.value.clone()));
        }
    }
    Ini {
        sections: sections
            .into_iter()
            .map(|(name, entries)| ini::Section { name, entries })
            .collect(),
    }
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    std::fs::write(path, text).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Copy every file under `src` into `dest`, recreating the directory
/// structure. A file already at the destination is overwritten, so calling
/// this for each data root in low to high order reproduces the shadowing
/// `Loadout::find` does. Symlinks are skipped rather than followed or copied
/// as links, since a packaged install should not depend on the source tree
/// staying where it was.
fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(src) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::Io {
                path: src.to_path_buf(),
                source,
            })
        }
    };
    for entry in entries {
        let entry = entry.map_err(|e| Error::Io {
            path: src.to_path_buf(),
            source: e,
        })?;
        let file_type = entry.file_type().map_err(|e| Error::Io {
            path: entry.path(),
            source: e,
        })?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            // A lower root may have left a plain file at this path. The
            // higher root's directory shadows it, so clear the file first.
            if dest_path.is_file() {
                std::fs::remove_file(&dest_path).map_err(|e| Error::Io {
                    path: dest_path.clone(),
                    source: e,
                })?;
            }
            copy_tree(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            copy_file_preserving(&entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Copy one file, keeping the source's permission bits (`fs::copy` already
/// does this on Unix) and modification time, so a mod's own timestamps do
/// not change just because it was packaged.
fn copy_file_preserving(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    // A lower root may have left a directory at this path (recursed into by
    // an earlier copy_tree call); the higher root's file shadows it, so
    // clear the directory first.
    if dest.is_dir() {
        std::fs::remove_dir_all(dest).map_err(|e| Error::Io {
            path: dest.to_path_buf(),
            source: e,
        })?;
    }
    // A file already at this path may be read-only, either a shadowed lower
    // root's file that kept its own read-only bit (mods and BSA-extracted
    // assets are often shipped that way) or this same package() call's own
    // prior output, which carries the read-only bit for the same reason.
    // Restore write access before overwriting it. fs::copy below reapplies
    // the new source's own permission bits right after, so the result still
    // ends up read-only when the source is, matching the doc comment above.
    restore_write_access(dest);
    std::fs::copy(src, dest).map_err(|e| Error::Io {
        path: dest.to_path_buf(),
        source: e,
    })?;
    if let Ok(meta) = std::fs::metadata(src) {
        if let Ok(modified) = meta.modified() {
            if let Ok(f) = std::fs::File::open(dest) {
                let _ = f.set_modified(modified);
            }
        }
    }
    Ok(())
}

/// If `path` exists and is not writable by its owner, add the owner-write
/// bit back. A no-op when the path is missing or already writable, so this
/// is safe to call before every overwrite rather than only the read-only
/// case.
#[cfg(unix)]
fn restore_write_access(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.mode() & 0o200 == 0 {
            perms.set_mode(perms.mode() | 0o200);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

#[cfg(not(unix))]
fn restore_write_access(_path: &Path) {}

/// Everything `resolve` needs, filled by a manager adapter or by hand.
#[derive(Debug, Clone)]
pub struct LoadoutInput {
    /// Holds `Skyrim_Default.ini` and `Skyrim.ccc`.
    pub game_dir: PathBuf,
    /// Lowest first.
    pub data_roots: Vec<PathBuf>,
    pub plugins_txt: Option<PathBuf>,
    pub loadorder_txt: Option<PathBuf>,
    /// Holds the three game INIs, `None` reads only `Skyrim_Default.ini`.
    pub ini_dir: Option<PathBuf>,
    /// Overrides `<game_dir>/Skyrim.ccc`.
    pub ccc: Option<PathBuf>,
}

impl LoadoutInput {
    /// The unmodded game: its Data, its `plugins.txt` from the local app data
    /// folder, its INIs from My Games. Either folder may be absent.
    pub fn vanilla(
        game_dir: &Path,
        my_games: Option<&Path>,
        local_appdata: Option<&Path>,
    ) -> LoadoutInput {
        LoadoutInput {
            game_dir: game_dir.to_path_buf(),
            data_roots: vec![game_dir.join("Data")],
            plugins_txt: local_appdata.map(|p| p.join("plugins.txt")),
            loadorder_txt: None,
            ini_dir: my_games.map(Path::to_path_buf),
            ccc: None,
        }
    }

    pub fn resolve(&self) -> Result<Loadout> {
        let mut problems = Vec::new();
        let files = discover(&self.data_roots);
        let mut available = Vec::new();
        for (_, path) in &files {
            match PluginHeader::read(path) {
                Ok(h) => available.push(h),
                Err(e) => problems.push(Problem::Unreadable {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    error: e.to_string(),
                }),
            }
        }
        let plugins_txt = match &self.plugins_txt {
            Some(p) => plugins_txt::read_plugins_txt(p)?,
            None => Vec::new(),
        };
        let loadorder_txt = match &self.loadorder_txt {
            Some(p) => plugins_txt::read_loadorder_txt(p)?,
            None => Vec::new(),
        };
        let ccc_path = self
            .ccc
            .clone()
            .unwrap_or_else(|| self.game_dir.join("Skyrim.ccc"));
        let early = ccc::early_loading(&ccc::read_ccc(&ccc_path));
        let mut load_order = order::resolve(&order::Inputs {
            available: &available,
            plugins_txt: &plugins_txt,
            loadorder_txt: &loadorder_txt,
            early: &early,
        })?;
        load_order.problems.extend(problems);

        let mut settings = Settings::new();
        apply_if_present(
            &mut settings,
            &Layer::Default,
            &self.game_dir.join("Skyrim_Default.ini"),
        )?;
        if let Some(dir) = &self.ini_dir {
            for (name, layer) in INI_NAMES
                .iter()
                .zip([Layer::Base, Layer::Custom, Layer::Prefs])
            {
                apply_if_present(&mut settings, &layer, &dir.join(name))?;
            }
        }
        let by_lower = lower_index(&self.data_roots);
        for e in load_order.active() {
            let ini_name = format!("{}.ini", stem(&e.name)).to_ascii_lowercase();
            if let Some(path) = by_lower.get(&ini_name) {
                apply_if_present(&mut settings, &Layer::Plugin(e.name.clone()), path)?;
            }
        }

        let mut archives = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in settings.archives() {
            if seen.insert(name.to_ascii_lowercase()) {
                archives.push(ArchiveRef {
                    path: by_lower.get(&name.to_ascii_lowercase()).cloned(),
                    name,
                    owner: None,
                });
            }
        }
        let mut names: Vec<&String> = by_lower.keys().collect();
        names.sort();
        for e in load_order.active() {
            let plain = format!("{}.bsa", stem(&e.name)).to_ascii_lowercase();
            let prefix = format!("{} - ", stem(&e.name)).to_ascii_lowercase();
            for n in &names {
                let has_bsa_ext = Path::new(n.as_str())
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("bsa"));
                let matches = **n == plain || (n.starts_with(&prefix) && has_bsa_ext);
                if matches && seen.insert((*n).clone()) {
                    archives.push(ArchiveRef {
                        name: by_lower[*n]
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| (*n).clone()),
                        path: Some(by_lower[*n].clone()),
                        owner: Some(e.name.clone()),
                    });
                }
            }
        }

        Ok(Loadout {
            data_roots: self.data_roots.clone(),
            load_order,
            settings,
            archives,
        })
    }
}

fn stem(name: &str) -> &str {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
}

fn apply_if_present(settings: &mut Settings, layer: &Layer, path: &Path) -> Result<()> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            settings.apply(layer, &Ini::parse(&text));
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Plugin files across the roots, later roots shadowing earlier ones by
/// name, in a stable order.
fn discover(roots: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if header::is_plugin_name(&name) && entry.path().is_file() {
                found.insert(name.to_ascii_lowercase(), entry.path());
            }
        }
    }
    let mut out: Vec<(String, PathBuf)> = found.into_iter().collect();
    out.sort();
    out
}

/// Every top-level file across the roots by lowercased name, later roots
/// winning.
fn lower_index(roots: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                found.insert(name, entry.path());
            }
        }
    }
    found
}

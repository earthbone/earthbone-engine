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
}

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

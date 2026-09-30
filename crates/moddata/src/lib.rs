// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! The configuration formats popular SKSE plugins invented, read natively.
//!
//! A large share of modern Skyrim mods carry no code. They ship an ini file or
//! a directory tree that one of four plugins reads, and without that plugin
//! the mod does nothing. `commonlib-shim` covers the function surface a DLL
//! calls, which does not help here, because the thing to be compatible with is
//! not a function but a file format. So the engine reads the formats itself
//! and the mods work with no plugin present:
//!
//! - [`spid`], Spell Perk Item Distributor, `*_DISTR.ini`, hands spells,
//!   perks, items, packages and outfits to actors by filter.
//! - [`kid`], Keyword Item Distributor, `*_KID.ini`, attaches keywords to
//!   records the same way.
//! - [`bos`], Base Object Swapper, `*_SWAP.ini`, swaps base objects on placed
//!   references.
//! - [`oar`], Open Animation Replacer, a directory tree, picks animations by
//!   condition.
//!
//! # Parsing and resolving are separate
//!
//! A configuration is read with no game data in hand, so a file that names a
//! plugin the user does not have still parses. [`resolve`] is the second step:
//! it takes a parsed file and a [`soe_esm::LoadOrder`] and produces
//! [`Patch`] values, each one a form and the change to make to it. Nothing
//! here touches a running world, and that hook belongs to the game crate.
//!
//! # What remains as written
//!
//! Keyword Item Distributor's traits field keeps its text because the grammar
//! differs by item type. Base Object Swapper property fields are parsed by
//! [`bos::Properties`] and applied by the game crate.

pub mod bos;
pub mod error;
pub mod filter;
pub mod form;
pub mod ini;
pub mod kid;
pub mod oar;
pub mod spid;

pub use error::{Error, Result};
pub use filter::Subject;
pub use form::FormRef;

use soe_esm::{GlobalFormId, LoadOrder};
use std::path::{Path, PathBuf};

/// What a configuration asks the engine to change about a form.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Attach a keyword, from Keyword Item Distributor.
    AddKeyword { keyword: GlobalFormId },
    /// Hand something to an actor, from Spell Perk Item Distributor.
    Distribute {
        record_type: spid::RecordType,
        what: GlobalFormId,
        count: spid::IndexOrCount,
        on_death: bool,
        /// Preserve SPID's trailing `!` through resolution.
        deterministic_chance: bool,
    },
    /// Replace a base object and optionally override its properties.
    SwapBase {
        to: GlobalFormId,
        properties: String,
    },
    /// Replace the base object of one named placed reference.
    SwapReference {
        to: GlobalFormId,
        properties: String,
    },
    /// Override a form or reference's properties, from Base Object Swapper.
    SetProperties { properties: String },
}

/// One resolved change: which form, what to do, how likely, and where it came
/// from so a conflict can be reported.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The form the change applies to. `None` when the rule selects by filter
    /// rather than naming a form, which is the usual case for a distribution.
    pub target: Option<GlobalFormId>,
    pub change: Change,
    /// Percent, 0 to 100.
    pub chance: f32,
    pub source: PathBuf,
    pub line: usize,
}

/// A file one of the three ini formats owns, found by its suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    Distr,
    Kid,
    Swap,
}

impl ConfigKind {
    /// The suffix the plugin matches on, before the extension.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Distr => "_DISTR",
            Self::Kid => "_KID",
            Self::Swap => "_SWAP",
        }
    }

    /// Which format a file name belongs to, if any. The plugins match the
    /// suffix case insensitively and only on `.ini`.
    pub fn of(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        let stem = lower.strip_suffix(".ini")?;
        [Self::Distr, Self::Kid, Self::Swap]
            .into_iter()
            .find(|kind| stem.ends_with(&kind.suffix().to_ascii_lowercase()))
    }
}

/// Every configuration file in a Data directory, sorted by name.
///
/// Sorting matters: all three plugins resolve a conflict by file name, and the
/// last file alphabetically wins. Base Object Swapper documents that plainly
/// and a third party tool exists only to override it.
///
/// # Errors
///
/// [`Error::Io`] when the directory cannot be listed.
pub fn discover(data_dir: impl AsRef<Path>) -> Result<Vec<(ConfigKind, PathBuf)>> {
    let dir = data_dir.as_ref();
    let entries = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    let mut out: Vec<(ConfigKind, PathBuf)> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            let kind = ConfigKind::of(name)?;
            Some((kind, path))
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

/// Turns a parsed keyword file into patches. A rule whose keyword does not
/// resolve is dropped, since there is nothing to attach.
pub fn resolve_kid(file: &kid::KidFile, order: &LoadOrder) -> Vec<Patch> {
    file.rules
        .iter()
        .filter_map(|rule| {
            let keyword = rule.keyword.resolve(order)?;
            Some(Patch {
                target: None,
                change: Change::AddKeyword { keyword },
                chance: rule.chance,
                source: file.path.clone(),
                line: rule.line,
            })
        })
        .collect()
}

/// Turns a parsed distribution file into patches. A line whose form does not
/// resolve is dropped, which is what the plugin reports and skips.
pub fn resolve_spid(file: &spid::DistrFile, order: &LoadOrder) -> Vec<Patch> {
    file.distributions
        .iter()
        .filter_map(|d| {
            let what = d.form.resolve(order)?;
            Some(Patch {
                target: None,
                change: Change::Distribute {
                    record_type: d.record_type,
                    what,
                    count: d.index_or_count,
                    on_death: d.on_death,
                    deterministic_chance: d.deterministic_chance,
                },
                chance: d.chance,
                source: file.path.clone(),
                line: d.line,
            })
        })
        .collect()
}

/// Turns a parsed swap file into patches, one per form on the left of a swap.
/// A set on the right resolves to its first present member, which is the
/// conservative reading: the plugin picks at random, and picking the first
/// keeps a resolve deterministic.
pub fn resolve_bos(file: &bos::SwapFile, order: &LoadOrder) -> Vec<Patch> {
    let mut out = Vec::new();
    for section in &file.sections {
        for swap in &section.swaps {
            let to = swap.to.iter().find_map(|f| f.resolve(order));
            for from in &swap.from {
                let Some(target) = from.resolve(order) else {
                    continue;
                };
                let change = match (section.kind, to) {
                    (bos::SwapKind::Properties, _) | (_, None) => Change::SetProperties {
                        properties: swap.properties.clone(),
                    },
                    (bos::SwapKind::References, Some(to)) => Change::SwapReference {
                        to,
                        properties: swap.properties.clone(),
                    },
                    (_, Some(to)) => Change::SwapBase {
                        to,
                        properties: swap.properties.clone(),
                    },
                };
                out.push(Patch {
                    target: Some(target),
                    change,
                    chance: swap.chance,
                    source: file.path.clone(),
                    line: swap.line,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{ConfigKind, Patch};

    #[test]
    fn a_file_name_names_its_format() {
        assert_eq!(ConfigKind::of("MyMod_DISTR.ini"), Some(ConfigKind::Distr));
        assert_eq!(ConfigKind::of("MyMod_KID.ini"), Some(ConfigKind::Kid));
        assert_eq!(ConfigKind::of("MyMod_SWAP.ini"), Some(ConfigKind::Swap));
    }

    #[test]
    fn the_suffix_match_ignores_case_and_needs_the_extension() {
        assert_eq!(ConfigKind::of("mymod_distr.ini"), Some(ConfigKind::Distr));
        assert_eq!(ConfigKind::of("MyMod_DISTR.txt"), None);
        assert_eq!(ConfigKind::of("Skyrim.ini"), None);
    }

    #[test]
    fn a_patch_is_small_enough_to_pass_around() {
        // Guards against the type growing a heap of owned data by accident.
        assert!(std::mem::size_of::<Patch>() <= 128);
    }
}

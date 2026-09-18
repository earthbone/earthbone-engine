// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every configuration the installed game actually carries.
//!
//! A vanilla install has none of these files, since all four formats belong to
//! mods. The tests below therefore skip on a clean install and say so, and they
//! start reporting the moment a mod that uses one is present. They exist
//! because every fixture in the rest of the suite is authored from the plugin
//! sources rather than taken from a shipped mod, which is the one gap
//! `docs/moddata.md` admits to.

use soe_moddata::{
    bos::SwapFile, discover, kid::KidFile, oar::AnimationMod, spid::DistrFile, ConfigKind,
};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: {} is not a directory", dir.display());
        None
    }
}

#[test]
fn every_installed_config_parses() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let found = discover(&dir).expect("the data directory lists");
    if found.is_empty() {
        eprintln!(
            "no _DISTR, _KID or _SWAP ini in {}, so this install carries no mod using them",
            dir.display()
        );
        return;
    }

    let (mut distr, mut kid, mut swap) = (0usize, 0usize, 0usize);
    for (kind, path) in &found {
        match kind {
            ConfigKind::Distr => {
                let f = DistrFile::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                eprintln!(
                    "  {} : {} distributions, {} unknown keys",
                    path.display(),
                    f.distributions.len(),
                    f.unknown_keys.len()
                );
                distr += f.distributions.len();
            }
            ConfigKind::Kid => {
                let f = KidFile::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                eprintln!(
                    "  {} : {} keyword rules, {} skipped",
                    path.display(),
                    f.rules.len(),
                    f.skipped.len()
                );
                kid += f.rules.len();
            }
            ConfigKind::Swap => {
                let f = SwapFile::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                eprintln!(
                    "  {} : {} sections, {} swaps",
                    path.display(),
                    f.sections.len(),
                    f.swaps().count()
                );
                swap += f.swaps().count();
            }
        }
    }
    eprintln!(
        "{} files: {distr} distributions, {kid} keyword rules, {swap} swaps",
        found.len()
    );
}

#[test]
fn every_installed_animation_tree_scans() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let meshes = dir.join("meshes");
    if !meshes.is_dir() {
        eprintln!("no loose meshes directory, so no animation mod is installed");
        return;
    }
    let found = AnimationMod::scan(&meshes).expect("the meshes tree scans");
    if found.sub_mods.is_empty() {
        eprintln!("no OAR or DAR submod under {}", meshes.display());
        return;
    }
    let legacy = found.sub_mods.iter().filter(|s| s.legacy).count();
    eprintln!(
        "{} submods, {legacy} legacy, {} conditions in all",
        found.sub_mods.len(),
        found
            .sub_mods
            .iter()
            .map(|s| s.conditions.len())
            .sum::<usize>()
    );
    // Priority order is the contract a caller relies on.
    for pair in found.sub_mods.windows(2) {
        assert!(
            pair[0].priority >= pair[1].priority,
            "submods must come back highest priority first"
        );
    }
}

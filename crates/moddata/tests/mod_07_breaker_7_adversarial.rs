// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_moddata::{
    bos::SwapFile, discover, kid::KidFile, oar::AnimationMod, spid::DistrFile, ConfigKind,
};
use std::path::{Path, PathBuf};

fn assert_source_line(path: &Path, line: usize) {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let source = contents
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or_else(|| panic!("{} has no physical line {line}", path.display()))
        .trim();
    assert!(!source.is_empty(), "{}:{line} is blank", path.display());
    assert!(
        !source.starts_with(';') && !source.starts_with('#') && !source.starts_with('['),
        "{}:{line} does not identify a config entry: {source}",
        path.display()
    );
}

#[test]
fn mod_07_real_rules_keep_exact_source_lines_across_the_corpus() {
    let Some(root) = std::env::var_os("SOE_MOD_CORPUS").map(PathBuf::from) else {
        eprintln!("SOE_MOD_CORPUS is unset, skipping real rule provenance audit");
        return;
    };
    assert!(root.is_dir(), "{} is not a directory", root.display());

    let mut counts = [0_usize; 4];
    for entry in std::fs::read_dir(&root).expect("read corpus root") {
        let mod_root = entry.expect("read corpus entry").path();
        if !mod_root.is_dir() {
            continue;
        }
        for (kind, path) in discover(&mod_root).expect("discover real configs") {
            match kind {
                ConfigKind::Distr => {
                    let file = DistrFile::read(&path).expect("parse real SPID file");
                    assert_eq!(file.path, path);
                    for rule in file.distributions {
                        assert_source_line(&path, rule.line);
                    }
                    counts[0] += 1;
                }
                ConfigKind::Kid => {
                    let file = KidFile::read(&path).expect("parse real KID file");
                    assert_eq!(file.path, path);
                    for rule in file.rules {
                        assert_source_line(&path, rule.line);
                    }
                    counts[1] += 1;
                }
                ConfigKind::Swap => {
                    let file = SwapFile::read(&path).expect("parse real BOS file");
                    assert_eq!(file.path, path);
                    for rule in file.swaps() {
                        assert_source_line(&path, rule.line);
                    }
                    counts[2] += 1;
                }
            }
        }
        let animation = AnimationMod::scan(&mod_root).expect("scan real OAR or DAR tree");
        for submod in animation.sub_mods {
            let user = submod.directory.join("user.json");
            let config = submod.directory.join("config.json");
            assert!(
                user.is_file() || config.is_file() || submod.legacy,
                "{} has no source config",
                submod.directory.display()
            );
            counts[3] += 1;
        }
    }

    assert!(counts.iter().all(|count| *count > 0), "fixture counts: {counts:?}");
}

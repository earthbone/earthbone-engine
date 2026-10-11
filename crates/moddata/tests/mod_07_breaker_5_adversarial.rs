// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_moddata::{
    bos::SwapFile, discover, kid::KidFile, spid::DistrFile, ConfigKind,
};
use std::path::PathBuf;

#[test]
fn mod_07_every_real_distributor_file_keeps_its_source_provenance() {
    let Some(root) = std::env::var_os("SOE_MOD_CORPUS").map(PathBuf::from) else {
        eprintln!("SOE_MOD_CORPUS is unset, skipping real distributor provenance survey");
        return;
    };
    assert!(root.is_dir(), "{} is not a directory", root.display());

    let mut file_count = 0;
    for entry in std::fs::read_dir(&root).expect("read corpus root") {
        let mod_root = entry.expect("read corpus entry").path();
        if !mod_root.is_dir() {
            continue;
        }
        for (kind, path) in discover(&mod_root).expect("discover mod config files") {
            file_count += 1;
            match kind {
                ConfigKind::Distr => {
                    let file = DistrFile::read(&path).expect("parse real SPID file");
                    assert!(!file.distributions.is_empty(), "{} has no rules", path.display());
                    assert_eq!(file.path, path);
                    assert!(file.distributions.iter().all(|rule| rule.line > 0));
                }
                ConfigKind::Kid => {
                    let file = KidFile::read(&path).expect("parse real KID file");
                    assert!(!file.rules.is_empty(), "{} has no rules", path.display());
                    assert_eq!(file.path, path);
                    assert!(file.rules.iter().all(|rule| rule.line > 0));
                }
                ConfigKind::Swap => {
                    let file = SwapFile::read(&path).expect("parse real BOS file");
                    assert!(file.swaps().next().is_some(), "{} has no swaps", path.display());
                    assert!(file.swaps().all(|rule| rule.line > 0));
                    assert_eq!(file.path, path);
                }
            }
        }
    }
    assert!(file_count > 0, "{} contains no distributor files", root.display());
}

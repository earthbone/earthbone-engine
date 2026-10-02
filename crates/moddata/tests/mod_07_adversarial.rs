// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{
    bos::SwapFile, ini::Ini, kid::KidFile, oar::AnimationMod, resolve_bos, resolve_kid,
    resolve_spid, spid::DistrFile, ConfigKind,
};
use std::path::{Path, PathBuf};

const KYWD: Tag = Tag::new(b"KYWD");
const SPEL: Tag = Tag::new(b"SPEL");

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(KYWD, 0x800).zsub(tag::EDID, "FixtureKeyword"));
    plugin.push_top(Record::new(SPEL, 0x801).zsub(tag::EDID, "FixtureSpell"));
    plugin.push_top(Record::new(tag::STAT, 0x802).zsub(tag::EDID, "FixtureRock"));
    plugin.push_top(Record::new(tag::STAT, 0x803).zsub(tag::EDID, "FixtureTree"));
    let mut load_order = LoadOrder::new();
    load_order
        .add("Fixture.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    load_order
}

#[test]
fn mod_07_kid_keeps_real_input_line_after_missing_form_is_dropped() {
    let parsed = KidFile::from_ini(&Ini::parse(
        "Fixture_KID.ini",
        "Keyword = Missing|Armor|Iron\n; authored fixture\nKeyword = FixtureKeyword|Armor|Iron\n",
    ));
    let patches = resolve_kid(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_KID.ini"));
    assert_eq!(patches[0].line, 3);
}

#[test]
fn mod_07_spid_keeps_real_input_line_after_missing_form_is_dropped() {
    let parsed = DistrFile::from_ini(&Ini::parse(
        "Fixture_DISTR.ini",
        "Spell = Missing|NONE\nSpell = FixtureSpell|NONE\n",
    ));
    let patches = resolve_spid(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_DISTR.ini"));
    assert_eq!(patches[0].line, 2);
}

#[test]
fn mod_07_bos_keeps_rule_provenance_when_replacement_is_missing() {
    let parsed = SwapFile::from_ini(&Ini::parse(
        "Fixture_SWAP.ini",
        "[Forms]\nFixtureRock|0x999~Absent.esp|scale(2)\n",
    ));
    let patches = resolve_bos(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_SWAP.ini"));
    assert_eq!(patches[0].line, 2);
}

fn files_below(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn mod_07_real_mod_parsers_keep_fixture_provenance() {
    let Some(root) = std::env::var_os("SOE_MOD_CORPUS").map(PathBuf::from) else {
        eprintln!("SOE_MOD_CORPUS is unset, skipping real mod distributor checks");
        return;
    };
    assert!(
        root.is_dir(),
        "SOE_MOD_CORPUS is not a directory: {}",
        root.display()
    );

    let mod_roots: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    assert!(
        !mod_roots.is_empty(),
        "{} contains no mod directories",
        root.display()
    );

    let mut ini_files = Vec::new();
    for mod_root in &mod_roots {
        for path in files_below(mod_root) {
            if let Some(kind) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(ConfigKind::of)
            {
                ini_files.push((kind, path));
            }
        }
    }
    ini_files.sort_by(|a, b| a.1.cmp(&b.1));

    let mut saw_distr = false;
    let mut saw_kid = false;
    let mut saw_swap = false;
    for (kind, path) in ini_files {
        match kind {
            ConfigKind::Distr if !saw_distr => {
                let file = DistrFile::read(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert!(
                    !file.distributions.is_empty(),
                    "{} has no parsed distributions",
                    path.display()
                );
                assert!(file.distributions.iter().all(|rule| rule.line > 0));
                assert_eq!(file.path, path);
                eprintln!(
                    "SPID fixture {} has {} rules",
                    path.display(),
                    file.distributions.len()
                );
                saw_distr = true;
            }
            ConfigKind::Kid if !saw_kid => {
                let file = KidFile::read(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert!(
                    !file.rules.is_empty(),
                    "{} has no parsed keyword rules",
                    path.display()
                );
                assert!(file.rules.iter().all(|rule| rule.line > 0));
                assert_eq!(file.path, path);
                eprintln!(
                    "KID fixture {} has {} rules",
                    path.display(),
                    file.rules.len()
                );
                saw_kid = true;
            }
            ConfigKind::Swap if !saw_swap => {
                let file = SwapFile::read(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert!(
                    file.swaps().next().is_some(),
                    "{} has no parsed swaps",
                    path.display()
                );
                assert!(file.swaps().all(|rule| rule.line > 0));
                assert_eq!(file.path, path);
                eprintln!("BOS fixture {} contains parsed swaps", path.display());
                saw_swap = true;
            }
            _ => {}
        }
    }

    let mut saw_oar = false;
    for mod_root in &mod_roots {
        let animation_mod = AnimationMod::scan(mod_root)
            .unwrap_or_else(|error| panic!("{}: {error}", mod_root.display()));
        if let Some(selected) = animation_mod.select_submod(|_| true) {
            let source = if selected.directory.join("user.json").is_file() {
                selected.directory.join("user.json")
            } else {
                selected.directory.join("config.json")
            };
            assert!(
                source.is_file(),
                "OAR fixture source is missing: {}",
                source.display()
            );
            eprintln!(
                "OAR fixture {} selected from {}",
                selected.name,
                source.display()
            );
            saw_oar = true;
            break;
        }
    }

    assert!(
        saw_distr,
        "SOE_MOD_CORPUS has no real SPID _DISTR.ini fixture"
    );
    assert!(saw_kid, "SOE_MOD_CORPUS has no real KID _KID.ini fixture");
    assert!(saw_swap, "SOE_MOD_CORPUS has no real BOS _SWAP.ini fixture");
    assert!(
        saw_oar,
        "SOE_MOD_CORPUS has no real OAR or DAR submod fixture"
    );
}

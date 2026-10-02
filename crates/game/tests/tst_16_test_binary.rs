// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Single-binary integration-test guards from the TST-16 backlog contract.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use crate::common::installed;
use soe_game::Game;

fn registration(name: &str) -> String {
    format!("#[path = \"{name}.rs\"]\nmod {name};")
}

fn registration_count(source: &str, name: &str) -> usize {
    let active = without_comments(source);
    let lines: Vec<_> = active
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let path = format!("#[path = \"{name}.rs\"]");
    let module = format!("mod {name};");
    lines
        .windows(2)
        .filter(|pair| pair[0] == path && pair[1] == module)
        .count()
}

fn without_comments(source: &str) -> String {
    let mut in_block_comment = false;
    let mut active = String::new();
    for line in source.lines() {
        let mut remaining = line;
        let mut code = String::new();
        while !remaining.is_empty() {
            if in_block_comment {
                if let Some(end) = remaining.find("*/") {
                    remaining = &remaining[end + 2..];
                    in_block_comment = false;
                } else {
                    break;
                }
            } else if let Some(start) = remaining.find("/*") {
                code.push_str(&remaining[..start]);
                remaining = &remaining[start + 2..];
                in_block_comment = true;
            } else if let Some(start) = remaining.find("//") {
                code.push_str(&remaining[..start]);
                break;
            } else {
                code.push_str(remaining);
                break;
            }
        }
        active.push_str(&code);
        active.push('\n');
    }
    active
}

fn missing_registrations<'a>(
    names: impl IntoIterator<Item = &'a str>,
    source: &str,
) -> Vec<String> {
    names
        .into_iter()
        .filter(|name| registration_count(source, name) == 0)
        .map(registration)
        .collect()
}

#[test]
fn tst_16_every_test_file_is_registered() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let source = std::fs::read_to_string(root.join("it.rs")).unwrap();
    let names: BTreeSet<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .map(|path| path.file_stem().unwrap().to_str().unwrap().to_owned())
        .filter(|name| name != "it")
        .collect();
    let missing = missing_registrations(names.iter().map(String::as_str), &source);
    assert!(
        missing.is_empty(),
        "test files are missing from tests/it.rs. Add:\n{}",
        missing.join("\n")
    );
    for name in names {
        assert_eq!(
            registration_count(&source, &name),
            1,
            "tests/it.rs must register {name}.rs exactly once"
        );
    }
}

#[test]
fn tst_16_missing_registration_names_the_declaration_to_add() {
    assert_eq!(
        missing_registrations(["existing", "new_case"], &registration("existing")),
        ["#[path = \"new_case.rs\"]\nmod new_case;"],
        "the guard must identify both the path and the module to add"
    );
    assert_eq!(
        missing_registrations(["existing"], "#[path = \"existing.rs\"]\nmod wrong;"),
        [registration("existing")],
        "a mismatched module name is not a registration"
    );
    assert_eq!(
        missing_registrations(
            ["existing"],
            "// #[path = \"existing.rs\"]\n// mod existing;"
        ),
        [registration("existing")],
        "a commented-out registration is not active Rust code"
    );
    assert_eq!(
        missing_registrations(
            ["existing"],
            "/* #[path = \"existing.rs\"]\nmod existing; */"
        ),
        [registration("existing")],
        "a block-commented registration is not active Rust code"
    );
}

#[test]
fn tst_16_child_test_names_keep_nested_modules_without_the_crate_prefix() {
    assert_eq!(
        crate::common::child_test_name("it::determinism", "quest_flow_hash_child"),
        "determinism::quest_flow_hash_child"
    );
    assert_eq!(
        crate::common::child_test_name("it::atomic_save::failures", "child_writer"),
        "atomic_save::failures::child_writer"
    );
    assert_eq!(
        crate::common::child_test_name("it::save_generations::failures", "child_writer"),
        "save_generations::failures::child_writer"
    );
}

#[test]
fn tst_16_test_names_match_the_pre_consolidation_inventory() {
    let before: BTreeSet<_> = include_str!("data/tst_16_before_test_names.txt")
        .lines()
        .map(str::to_owned)
        .collect();
    let after: BTreeSet<_> = include_str!("data/tst_16_after_test_names.txt")
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(before.len(), 2_289, "pre-change inventory size changed");
    assert_eq!(after.len(), 2_289, "post-change inventory size changed");
    let missing: Vec<_> = before.difference(&after).cloned().collect();
    let added: Vec<_> = after.difference(&before).cloned().collect();
    assert!(
        missing.is_empty() && added.is_empty(),
        "before and after test-name inventories differ: missing {missing:?}, added {added:?}"
    );
}

#[test]
fn tst_16_manifest_selects_one_integration_binary() {
    let manifest = include_str!("../Cargo.toml");
    let package = manifest
        .split("[package]")
        .nth(1)
        .unwrap()
        .split("[lib]")
        .next()
        .unwrap();
    assert!(
        package.lines().any(|line| line == "autotests = false"),
        "automatic integration-test discovery must be disabled"
    );
    assert_eq!(manifest.matches("[[test]]").count(), 1);
    let target = manifest
        .split("[[test]]")
        .nth(1)
        .unwrap()
        .split("[features]")
        .next()
        .unwrap();
    assert!(target.lines().any(|line| line == "name = \"it\""));
    assert!(target.lines().any(|line| line == "path = \"tests/it.rs\""));
}

#[test]
fn tst_16_registration_uses_union_merge_and_writer_instructions() {
    let attributes = include_str!("../../../.gitattributes");
    assert!(
        attributes
            .lines()
            .any(|line| line == "crates/game/tests/it.rs merge=union")
    );
    for instructions in [
        include_str!("../../../AGENTS.md"),
        include_str!("../../../CLAUDE.md"),
    ] {
        assert!(instructions.contains("#[path = \"<name>.rs\"]"));
        assert!(instructions.contains("mod <name>;"));
        assert!(instructions.contains("crates/game/tests/it.rs"));
    }
}

fn check_helper_imports(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            check_helper_imports(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|extension| extension == "rs")
        && path.file_name().unwrap() != "mod.rs"
    {
        let source = std::fs::read_to_string(path).unwrap();
        for helper in [
            "installed",
            "legacy",
            "mag_adversarial",
            "save_tail",
            "magic_adversarial",
        ] {
            let declaration = format!("mod {helper};");
            assert!(
                !source.lines().any(|line| line.trim() == declaration),
                "{} must import {helper} through crate:: instead of redeclaring it",
                path.display()
            );
        }
        for directory in ["common", "support"] {
            let declaration = format!("#[path = \"{directory}/");
            assert!(
                !source
                    .lines()
                    .any(|line| line.trim().starts_with(&declaration)),
                "{} must import shared helpers through crate::",
                path.display()
            );
        }
    }
}

#[test]
fn tst_16_shared_helpers_are_declared_once() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let source = std::fs::read_to_string(root.join("it.rs")).unwrap();
    for module in ["common", "support"] {
        assert_eq!(source.matches(&format!("mod {module};")).count(), 1);
    }
    check_helper_imports(&root);
}

#[test]
fn tst_16_installed_callers_share_five_masters_and_keep_worlds_independent() {
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let Some(first) = installed::load_order(&masters) else {
        eprintln!("SKYRIM_DATA not set, skipping installed shared-binary check");
        return;
    };
    let second = crate::common::installed::load_order(&masters).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(
        first
            .files()
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>(),
        masters
    );
    let first = Game::from_shared_order(first);
    let second = Game::from_shared_order(second);
    let before = second.world().save_state();
    first.world_mut().perk_points = 16;
    assert_eq!(second.world().save_state(), before);
}

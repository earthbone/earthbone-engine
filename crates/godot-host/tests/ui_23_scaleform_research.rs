// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn action_script_root() -> PathBuf {
    std::env::var_os("SOE_SKYUI_ACTIONSCRIPT").map_or_else(
        || {
            PathBuf::from(
                "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/SkyUI-Community/source/actionscript",
            )
        },
        PathBuf::from,
    )
}

fn action_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            action_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "as") {
            files.push(path);
        }
    }
    files.sort();
    Ok(())
}

fn delegate_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (index, _) in source.match_indices("gfx.io.GameDelegate.") {
        let operation = &source[index + "gfx.io.GameDelegate.".len()..];
        let Some((method, rest)) = operation.split_once('(') else {
            continue;
        };
        if !matches!(method, "call" | "addCallBack") {
            continue;
        }
        let rest = rest.trim_start();
        let Some(quote) = rest
            .chars()
            .next()
            .filter(|character| matches!(character, '\'' | '"'))
        else {
            continue;
        };
        if let Some(name) = rest[1..]
            .split(quote)
            .next()
            .filter(|name| !name.is_empty())
        {
            names.insert(name.to_owned());
        }
    }
    names
}

#[test]
fn ui_23_scaleform_research_documents_tags_and_extensions() {
    let doc = std::fs::read_to_string(repo_root().join("docs/ui-frameworks.md")).unwrap();
    for term in [
        "`GFX` and `CFX`",
        "`DefineShape`",
        "`DefineSprite`",
        "`DoAction`",
        "gfx.io.GameDelegate.call",
        "gfx.ui.NavigationCode",
        "gfx.managers.FocusHandler",
        "gfx.managers.InputDelegate",
        "gfx.controls.ButtonGroup",
        "gfx.events.EventDispatcher",
        "Scaleform-only APIs",
        "Ruffle gaps for Skyrim menus",
        "Ruffle repository",
        "provide the Skyrim `gfx` namespace",
        "Skyrim `gfx` namespace",
        "GameDelegate",
        "It does not add",
        "Scaleform ActionScript APIs",
        "_global.gfxExtensions",
        "_global.gfxPlayer",
    ] {
        assert!(doc.contains(term), "research doc is missing {term}");
    }
    assert!(doc.contains("153 `.as` files"));
    assert!(doc.contains("337 `call` sites"));
    assert!(doc.contains("133 `addCallBack` sites"));
}

#[test]
fn ui_23_ruffle_api_gaps_are_checked_at_a_pinned_revision() {
    let doc = std::fs::read_to_string(repo_root().join("docs/ui-frameworks.md")).unwrap();
    for required in [
        "Ruffle nightly 2026-10-09 at commit `fbb4ce8`",
        "Its AVM1 global registration has no `gfx` namespace",
        "`gfx.io`, `gfx.ui`,",
        "`gfx.managers`, `gfx.controls` and `gfx.events`",
        "The same revision's AVM1 `TextField` implementation has no `SetText`",
        "`TextField.SetText` is a missing Scaleform extension",
        "This check does not mark standard Flash methods such as `MovieClip.gotoAndStop` as",
        "https://github.com/ruffle-rs/ruffle/blob/fbb4ce8/core/src/avm1/globals.rs",
        "https://github.com/ruffle-rs/ruffle/blob/fbb4ce8/core/src/avm1/globals/text_field.rs",
    ] {
        assert!(
            doc.contains(required),
            "Ruffle API audit is missing {required}"
        );
    }
    assert!(!doc.contains("Method-level API gaps\n  therefore remain unverified"));
}

#[test]
fn ui_23_scaleform_research_covers_vanilla_delegate_calls() {
    let source_root = action_script_root();
    if !source_root.exists() {
        eprintln!("SkyUI ActionScript source is unavailable, skipped");
        return;
    }
    let doc = std::fs::read_to_string(repo_root().join("docs/ui-frameworks.md")).unwrap();
    for folder in ["Vanilla", "PauseMenu"] {
        let mut files = Vec::new();
        action_files(&source_root.join(folder), &mut files).unwrap();
        for file in files {
            let source = std::fs::read_to_string(&file).unwrap();
            let names = delegate_names(&source);
            if names.is_empty() {
                continue;
            }
            let stem = file.file_stem().unwrap().to_string_lossy();
            let class = format!("`{folder}/{stem}`");
            let row = doc
                .lines()
                .find(|line| line.contains(&class))
                .unwrap_or_else(|| panic!("missing source class {class}"));
            for name in names {
                let api = format!("`{name}`");
                assert!(row.contains(&api), "{class} is missing {api}");
            }
        }
    }
}

#[test]
fn ui_23_scaleform_research_estimates_and_splits_followup_work() {
    let doc = std::fs::read_to_string(repo_root().join("docs/ui-frameworks.md")).unwrap();
    for term in [
        "| Pin Ruffle, build an embedded runtime, and render one static vanilla SWF | 1 to 2 weeks |",
        "| Implement the used GFx extension subset | 3 to 5 weeks |",
        "| Bridge vanilla GameDelegate operations | 6 to 10 weeks |",
        "| Run vanilla and mod compatibility surveys | 3 to 5 weeks |",
        "| Load-order runtime selection and diagnostics | 1 to 2 weeks |",
        "**UI-27:",
        "**UI-28:",
        "**UI-29:",
        "**UI-30:",
    ] {
        assert!(doc.contains(term), "research doc is missing {term}");
    }
}

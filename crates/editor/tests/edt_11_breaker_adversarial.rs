// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

#[test]
fn edt_11_chain_resolves_owner_through_reordered_master_table() {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let other_path = directory.path().join("Other.esm");
    let patch_path = directory.path().join("Patch.esp");

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "BaseVersion"));
    let other = Plugin::new(FileHeader::default());
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Other.esm".to_string(), "Base.esm".to_string()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"STAT"), 0x0100_0800).zsub(Tag(*b"EDID"), "PatchVersion"));

    std::fs::write(&base_path, base.to_bytes()).unwrap();
    std::fs::write(&other_path, other.to_bytes()).unwrap();
    std::fs::write(&patch_path, patch.to_bytes()).unwrap();
    let document = EditorDocument::open(&[base_path, other_path, patch_path], "Patch.esp").unwrap();

    let chain = document.override_chain(GlobalFormId {
        file: 0,
        object: 0x800,
    });
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].plugin_name, "Base.esm");
    assert_eq!(chain[0].record.editor_id().as_deref(), Some("BaseVersion"));
    assert_eq!(chain[1].plugin_name, "Patch.esp");
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("PatchVersion"));
    assert!(!chain[0].is_winner);
    assert!(chain[1].is_winner);
}

#[test]
fn edt_11_installed_conflict_chain_follows_plugin_priority() {
    let Some(data_path) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let data_path = std::path::PathBuf::from(data_path);
    let paths = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]
    .map(|name| data_path.join(name));
    let document = EditorDocument::open(&paths, "Dragonborn.esm").unwrap();

    let conflict = document
        .search_forms(None, "")
        .find_map(|form| {
            let chain = document.override_chain(form);
            (chain.len() > 1).then_some(chain)
        })
        .expect("installed master load order should contain an overridden form");
    let positions = conflict
        .iter()
        .map(|version| {
            document
                .load_order()
                .position_of(version.plugin_name)
                .unwrap()
        })
        .collect::<Vec<_>>();

    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(conflict.last().unwrap().is_winner);
    assert!(conflict[..conflict.len() - 1]
        .iter()
        .all(|version| !version.is_winner));
}

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("tst_12_mod_corpus.rs");

#[test]
fn tst_12_breaker4_resolves_assets_across_the_installed_modlist_case_insensitively() {
    let temp = TempDir::new("shared-asset");
    let plugin_mod = temp.0.join("PluginMod");
    let asset_mod = temp.0.join("AssetProvider");
    std::fs::create_dir_all(&plugin_mod).unwrap();
    std::fs::create_dir_all(asset_mod.join("meshes/custom")).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800).zsub(Tag::new(b"MODL"), "Meshes/Custom/Shared.NIF"),
    );
    std::fs::write(plugin_mod.join("Plugin.esp"), plugin.to_bytes()).unwrap();
    std::fs::write(asset_mod.join("meshes/custom/shared.nif"), b"present").unwrap();

    let reports = run_corpus(&temp.0);
    let report = reports
        .iter()
        .find(|report| report.name == "PluginMod")
        .unwrap();
    assert!(report.missing_assets.is_empty(), "{report:?}");
}

#[test]
fn tst_12_breaker4_keeps_identical_missing_assets_scoped_to_each_mod() {
    let temp = TempDir::new("mod-isolation");
    for name in ["AlphaMod", "ZuluMod"] {
        let mod_dir = temp.0.join(name);
        std::fs::create_dir_all(&mod_dir).unwrap();
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag::new(b"STAT"), 0x800)
                .zsub(Tag::new(b"MODL"), "meshes/shared/missing.nif"),
        );
        std::fs::write(mod_dir.join(format!("{name}.esp")), plugin.to_bytes()).unwrap();
    }

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 2);
    for report in &reports {
        assert_eq!(report.missing_assets.len(), 1, "{report:?}");
        assert!(report.missing_assets[0].contains("meshes/shared/missing.nif"));
    }
}

#[test]
fn tst_12_breaker4_deduplicates_missing_references_after_path_case_folding() {
    let temp = TempDir::new("duplicate-case-path");
    let mod_dir = temp.0.join("DuplicateReferences");
    std::fs::create_dir_all(&mod_dir).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let record = Record::new(Tag::new(b"STAT"), 0x800)
        .zsub(Tag::new(b"MODL"), "Meshes/Shared/Missing.NIF")
        .zsub(Tag::new(b"DMDL"), "meshes/shared/missing.nif");
    plugin.push_top(record);
    std::fs::write(mod_dir.join("DuplicateReferences.esp"), plugin.to_bytes()).unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].missing_assets.len(), 1, "{:?}", reports[0]);
}

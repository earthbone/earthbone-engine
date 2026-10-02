// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("tst_12_mod_corpus.rs");

#[test]
fn tst_12_breaker_mixed_children_keep_each_mod_report_separate() {
    let temp = TempDir::new("mixed-mods");
    let broken = temp.0.join("BrokenPack");
    let missing = temp.0.join("MissingPack");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::create_dir_all(&missing).unwrap();
    std::fs::write(broken.join("bad.esp"), b"not a plugin").unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800).zsub(Tag::new(b"MODL"), "meshes/absent/item.nif"),
    );
    std::fs::write(missing.join("MissingPack.esp"), plugin.to_bytes()).unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(
        reports
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>(),
        ["BrokenPack", "MissingPack"]
    );
    assert_eq!(reports[0].parser_failures.len(), 1);
    assert!(reports[0].missing_assets.is_empty());
    assert!(reports[1].parser_failures.is_empty());
    assert_eq!(reports[1].missing_assets.len(), 1);
}

#[test]
fn tst_12_breaker_bad_archive_is_reported_under_its_own_mod() {
    let temp = TempDir::new("bad-archive");
    let mod_dir = temp.0.join("ArchivePack");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(mod_dir.join("broken.bsa"), b"not a BSA archive").unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "ArchivePack");
    assert_eq!(reports[0].parser_failures.len(), 1);
    assert!(reports[0].parser_failures[0].contains("broken.bsa"));
}

#[test]
fn tst_12_breaker_reports_missing_assets_with_leading_data_separator() {
    let temp = TempDir::new("leading-data-separator");
    let mod_dir = temp.0.join("RootedAssetPack");
    std::fs::create_dir_all(&mod_dir).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800)
            .zsub(Tag::new(b"MODL"), "\\meshes/custom/missing.nif"),
    );
    std::fs::write(mod_dir.join("RootedAssetPack.esp"), plugin.to_bytes()).unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "RootedAssetPack");
    assert_eq!(reports[0].missing_assets.len(), 1);
    assert!(reports[0].missing_assets[0].contains("meshes/custom/missing.nif"));
}

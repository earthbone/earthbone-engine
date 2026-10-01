// SPDX-License-Identifier: GPL-3.0-or-later

include!("tst_12_mod_corpus.rs");

#[test]
fn tst_12_additional_ignores_asset_like_editor_ids_and_names() {
    let temp = TempDir::new("non_asset_strings");
    let mod_dir = temp.0.join("TextOnlyMod");
    std::fs::create_dir_all(&mod_dir).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800)
            .zsub(Tag::new(b"EDID"), "meshes/identifier.nif")
            .zsub(Tag::new(b"FULL"), "textures/example.dds"),
    );
    std::fs::write(mod_dir.join("TextOnly.esp"), plugin.to_bytes()).unwrap();

    let report = scan_mod(&mod_dir, &BTreeSet::new());
    assert!(
        report.missing_assets.is_empty(),
        "identifier and display strings are not asset paths: {:?}",
        report.missing_assets
    );
}

#[test]
fn tst_12_additional_rejects_an_empty_child_mod_corpus() {
    let temp = TempDir::new("empty_corpus");
    assert!(
        mod_roots(&temp.0).is_empty(),
        "the configured corpus requires one mod directory per child"
    );
}

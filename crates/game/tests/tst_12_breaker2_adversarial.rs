// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("tst_12_mod_corpus.rs");

#[test]
fn tst_12_breaker2_assets_resolve_across_mod_directories_case_insensitively() {
    let temp = TempDir::new("cross-mod-asset");
    let referring = temp.0.join("ReferringMod");
    let providing = temp.0.join("ProvidingMod");
    std::fs::create_dir_all(&referring).unwrap();
    std::fs::create_dir_all(providing.join("Meshes/Custom")).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800)
            .zsub(Tag::new(b"MODL"), "meshes/custom/shared.nif"),
    );
    std::fs::write(referring.join("Referring.esp"), plugin.to_bytes()).unwrap();
    std::fs::write(providing.join("Meshes/Custom/Shared.NIF"), b"present").unwrap();

    let reports = run_corpus(&temp.0);
    let referring_report = reports
        .iter()
        .find(|report| report.name == "ReferringMod")
        .unwrap();
    assert!(
        referring_report.missing_assets.is_empty(),
        "installed asset from another mod was reported missing: {:?}",
        referring_report.missing_assets
    );
}

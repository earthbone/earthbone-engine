// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("tst_12_mod_corpus.rs");

#[test]
fn tst_12_breaker3_reports_truncated_ogg_streams() {
    let temp = TempDir::new("truncated-ogg");
    let mod_dir = temp.0.join("TruncatedAudioMod");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(mod_dir.join("truncated.ogg"), b"OggS").unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "TruncatedAudioMod");
    assert_eq!(reports[0].files_scanned, 1);
    assert_eq!(reports[0].parser_failures.len(), 1);
    assert!(reports[0].parser_failures[0].contains("truncated.ogg"));
}

#[test]
fn tst_12_breaker3_matches_ogg_extensions_without_case_sensitivity() {
    let temp = TempDir::new("uppercase-ogg");
    let mod_dir = temp.0.join("UppercaseAudioMod");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(mod_dir.join("broken.OGG"), b"not an Ogg stream").unwrap();

    let reports = run_corpus(&temp.0);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "UppercaseAudioMod");
    assert_eq!(reports[0].files_scanned, 1);
    assert_eq!(reports[0].parser_failures.len(), 1);
    assert!(reports[0].parser_failures[0].contains("broken.OGG"));
}

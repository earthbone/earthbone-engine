// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PackageData coverage ledger using the strict reader's failure offsets.
//! Source limits and ledger schema are in docs/save-package-variants.md.

use soe_save::{Error, Save};
use std::path::{Path, PathBuf};

const MANIFEST: &str = include_str!("../../../docs/save-package-variants-v1.tsv");
const VERSION: &str = "manifest\tsoe-package-variants\t1\n";

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

fn classify(error: Error, payload: &[u8]) -> (&'static str, usize) {
    let Error::Reference { reason } = error else {
        panic!("unclassified decoder error: {error}");
    };
    let (description, offset) = reason.rsplit_once(" at ").unwrap();
    let offset: usize = offset.parse().unwrap();
    let (variant, name): (_, &[u8]) = match description {
        "unsupported PackageData kind 18 procedure \"Sequence\"" => ("PD18-SEQUENCE", b"Sequence"),
        "unsupported PackageData kind 18 procedure \"Stacked\"" => ("PD18-STACKED", b"Stacked"),
        _ => panic!("unclassified decoder error: {reason}"),
    };
    // CONTEXT: Read only the already consumed header, never search the opaque suffix.
    let mut header = vec![108, 18, 0];
    header.extend(u16::try_from(name.len()).unwrap().to_le_bytes());
    header.extend(name);
    assert_eq!(
        payload.get(offset..offset + header.len()),
        Some(header.as_slice())
    );
    (variant, offset)
}

#[test]
fn sav_22_manifest_classifies_every_corpus_package_failure() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping package manifest corpus");
        return;
    };
    let root = Path::new(&root);
    let mut paths = vec![];
    collect(root, &mut paths);
    paths.sort();
    assert_eq!(
        paths.len(),
        60,
        "version 1 requires the complete known corpus"
    );
    let mut rows = vec![];
    let (mut references, mut decoded, mut sequence, mut stacked) = (0, 0, 0, 0);
    for path in paths {
        let name = path.strip_prefix(root).unwrap().to_str().unwrap();
        assert!(!name.contains(['\t', '\n', '\r']));
        let save = Save::open(&path).unwrap();
        assert_eq!(save.form_version, 78);
        let (mut file_references, mut file_failures) = (0, 0);
        for form in &save.change_forms {
            match form.reference_contents(save.form_version) {
                Ok(None) => continue,
                Ok(Some(contents)) => {
                    let payload = form.payload().unwrap();
                    assert_eq!(
                        contents.data_bytes().unwrap(),
                        payload[contents.start..contents.end]
                    );
                    decoded += 1;
                }
                Err(error) => {
                    let payload = form.payload().unwrap();
                    let (variant, offset) = classify(error, &payload);
                    match variant {
                        "PD18-SEQUENCE" => sequence += 1,
                        "PD18-STACKED" => stacked += 1,
                        _ => unreachable!(),
                    }
                    rows.push(format!(
                        "occurrence\t{name}\t{:06x}\t{:08x}\t{:08x}\t{offset}\t{}\t{variant}\tOPAQUE-CONTINUATION",
                        form.form.raw(), save.resolve(form.form).unwrap(), form.change_flags, payload.len(),
                    ));
                    file_failures += 1;
                }
            }
            file_references += 1;
        }
        references += file_references;
        rows.push(format!("save\t{name}\t{file_references}\t{file_failures}"));
    }
    // CONTEXT: These counts were fixed from the pre-existing strict corpus baseline.
    assert_eq!((references, decoded), (1_154_526, 1_154_445));
    assert_eq!((sequence, stacked), (72, 9));
    rows.sort();
    let actual = format!(
        "{VERSION}corpus\t60\t{references}\t{decoded}\nvariant\tPD18-SEQUENCE\t{sequence}\tunsupported\tSEQUENCE-LAYOUT\nvariant\tPD18-STACKED\t{stacked}\tunsupported\tSTACKED-LAYOUT\n{}\n",
        rows.join("\n")
    );
    if actual != MANIFEST {
        eprintln!("PACKAGE-MANIFEST-BEGIN\n{actual}PACKAGE-MANIFEST-END");
    }
    assert_eq!(actual, MANIFEST, "versioned package coverage changed");
    eprintln!("SAV-22: 60 saves, {references} reference sections, {decoded} exact reencodings, {sequence} Sequence and {stacked} Stacked blockers classified");
}

#[test]
fn unknown_procedure_is_not_covered_by_a_known_variant() {
    let error = Error::Reference {
        reason: "unsupported PackageData kind 18 procedure \"Other\" at 0".into(),
    };
    assert!(std::panic::catch_unwind(|| classify(error, &[108, 18, 0])).is_err());
}

#[test]
fn manifest_occurrences_have_a_reason_and_bounded_enclosing_record() {
    assert!(MANIFEST.starts_with(VERSION));
    let mut occurrences = 0;
    for row in MANIFEST
        .lines()
        .filter(|line| line.starts_with("occurrence\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 9);
        let offset: usize = fields[5].parse().unwrap();
        let end: usize = fields[6].parse().unwrap();
        let header_len = match fields[7] {
            "PD18-SEQUENCE" => 13,
            "PD18-STACKED" => 12,
            unknown => panic!("unclassified variant {unknown}"),
        };
        assert!(
            end > offset + header_len,
            "named state must have an opaque suffix"
        );
        assert_eq!(fields[8], "OPAQUE-CONTINUATION");
        occurrences += 1;
    }
    assert_eq!(occurrences, 81);
}

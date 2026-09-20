// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-06 source and corpus coverage, not actor-modifier decoding.
//! Flag names follow xEdit and UESP ACHR. Modifier field order is unknown.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use soe_save::{reference::Initial, RefKind, Save};

const MODIFIER_BITS: [(u8, &str); 5] = [
    (19, "disposition"),
    (20, "temporary"),
    (21, "damage"),
    (22, "override"),
    (23, "permanent"),
];
const MODIFIER_MASK: u32 = 0x00F8_0000;

#[derive(Debug, Default)]
struct Coverage {
    records: usize,
    decoded_prefixes: usize,
    blocked_prefixes: usize,
    nonempty_suffixes: usize,
    suffix_bytes: usize,
    animation_flag: usize,
    created_references: usize,
    explicit_created_bases: usize,
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_06_corpus_reports_modifier_flags_and_prefix_coverage() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping actor modifier coverage survey");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "coverage survey requires at least 50 saves"
    );
    let mut variants = BTreeMap::<(u8, u8, u32), Coverage>::new();
    let mut flags = BTreeMap::<u8, usize>::new();
    let mut errors = BTreeMap::<(u32, String), usize>::new();
    let mut player_masks = BTreeMap::<u32, usize>::new();
    let mut records = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mut players = 0;
        for form in &save.change_forms {
            if form.type_index() != 1 {
                continue;
            }
            records += 1;
            let mask = form.change_flags & MODIFIER_MASK;
            let coverage = variants
                .entry((save.form_version, form.version, mask))
                .or_default();
            coverage.records += 1;
            for (bit, _) in MODIFIER_BITS {
                if form.change_flags & (1 << bit) != 0 {
                    *flags.entry(bit).or_default() += 1;
                }
            }
            if save.resolve(form.form) == Some(0x14) {
                players += 1;
                *player_masks.entry(mask).or_default() += 1;
            }
            coverage.animation_flag += usize::from(form.change_flags & (1 << 28) != 0);
            coverage.created_references += usize::from(form.form.kind() == RefKind::Created);
            let payload = form.payload().unwrap();
            let header = form.reference_header(save.form_version).unwrap().unwrap();
            assert_eq!(
                header.data_bytes().unwrap(),
                payload[..header.bytes_consumed]
            );
            let initial_base = match header.initial {
                Initial::Created { base, .. } => Some(base),
                _ => None,
            };
            if let Some(base) = header.base.or(initial_base) {
                let resolved = save
                    .resolve(base)
                    .expect("explicit actor base must resolve");
                coverage.explicit_created_bases += usize::from(resolved >> 24 == 0xFF);
            }
            match form.reference_contents(save.form_version) {
                Ok(Some(contents)) => {
                    coverage.decoded_prefixes += 1;
                    assert_eq!(
                        contents.data_bytes().unwrap(),
                        payload[contents.start..contents.end]
                    );
                    assert_eq!(contents.end + contents.remaining_bytes, payload.len());
                    coverage.nonempty_suffixes += usize::from(contents.remaining_bytes != 0);
                    coverage.suffix_bytes += contents.remaining_bytes;
                }
                Ok(None) => panic!("ACHR record must expose reference contents"),
                Err(error) => {
                    coverage.blocked_prefixes += 1;
                    *errors.entry((mask, error.to_string())).or_default() += 1;
                }
            }
        }
        assert_eq!(players, 1, "one PlayerRef ACHR required per save");
    }
    assert!(
        records > 100_000,
        "coverage survey requires a large actor corpus"
    );
    assert_eq!(variants.values().map(|v| v.records).sum::<usize>(), records);
    for coverage in variants.values() {
        assert_eq!(
            coverage.decoded_prefixes + coverage.blocked_prefixes,
            coverage.records
        );
    }
    eprintln!(
        "SAV-06 coverage schema 1: {} saves, {records} ACHR records",
        paths.len()
    );
    for (bit, name) in MODIFIER_BITS {
        eprintln!(
            "bit={bit} domain={name} records={}",
            flags.get(&bit).copied().unwrap_or(0)
        );
    }
    for ((save_version, change_version, mask), coverage) in variants {
        eprintln!("save_version={save_version} change_version={change_version} modifier_mask={mask:#010x} {coverage:?}");
    }
    for ((mask, error), count) in errors {
        eprintln!("blocked modifier_mask={mask:#010x} count={count} error={error}");
    }
    for (mask, count) in player_masks {
        eprintln!("player modifier_mask={mask:#010x} count={count}");
    }
    eprintln!("Suffix bytes include unparsed animation and actor data. No modifier field boundary or component mapping is established by this survey.");
}

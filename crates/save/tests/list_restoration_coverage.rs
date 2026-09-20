// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-52 field and identity coverage. Counts do not establish runtime semantics.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use soe_save::{RefId, Save};

#[derive(Debug, Default)]
struct Coverage {
    records: usize,
    form_flags: usize,
    absent_lists: usize,
    empty_lists: usize,
    entries: usize,
    null_entries: usize,
    created_entries: usize,
    unresolved_entries: usize,
    duplicate_form_ids: usize,
}

impl Coverage {
    fn identity(&mut self, save: &Save, form: RefId, ids: &mut BTreeSet<u32>) {
        self.entries += 1;
        match save.resolve(form) {
            Some(id) => {
                self.null_entries += usize::from(id == 0);
                self.created_entries += usize::from(id >> 24 == 0xFF);
                self.duplicate_form_ids += usize::from(!ids.insert(id));
            }
            None => self.unresolved_entries += 1,
        }
    }
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
fn sav_52_corpus_accounts_for_list_fields_without_assigning_chance_semantics() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping list restoration coverage");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "coverage requires at least 50 saves");
    let mut coverage: BTreeMap<_, Coverage> = ["FLST", "LVLN", "LVLI", "LVSP"]
        .into_iter()
        .map(|tag| (tag, Coverage::default()))
        .collect();
    let mut fields = BTreeMap::new();
    let mut variants = BTreeMap::new();
    let mut masks = BTreeMap::new();
    let mut targets = BTreeMap::new();
    let mut bytes = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let tag = form.record_type();
            let Some(coverage) = coverage.get_mut(tag) else {
                continue;
            };
            coverage.records += 1;
            *variants
                .entry((save.form_version, form.version, tag, form.change_flags))
                .or_insert(0usize) += 1;
            *targets
                .entry((tag, save.resolve(form.form)))
                .or_insert(0usize) += 1;
            let mut ids = BTreeSet::new();
            let (flags, encoded) = if tag == "FLST" {
                let change = form.form_list_change().unwrap().unwrap();
                let encoded = change.data_bytes().unwrap();
                match change.forms {
                    Some(forms) => {
                        coverage.empty_lists += usize::from(forms.is_empty());
                        for item in forms {
                            coverage.identity(&save, item, &mut ids);
                        }
                    }
                    None => coverage.absent_lists += 1,
                }
                (change.form_flags, encoded)
            } else {
                let change = form.leveled_list_change().unwrap().unwrap();
                let encoded = change.data_bytes().unwrap();
                match change.entries {
                    Some(entries) => {
                        coverage.empty_lists += usize::from(entries.is_empty());
                        for entry in entries {
                            coverage.identity(&save, entry.form, &mut ids);
                            *fields
                                .entry((tag, entry.level, entry.count, entry.chance))
                                .or_insert(0usize) += 1;
                        }
                    }
                    None => coverage.absent_lists += 1,
                }
                (change.form_flags, encoded)
            };
            if let Some(flags) = flags {
                coverage.form_flags += 1;
                *masks
                    .entry((tag, flags.flags, flags.unknown))
                    .or_insert(0usize) += 1;
            }
            assert_eq!(encoded, form.payload().unwrap().as_ref());
            bytes += encoded.len();
        }
    }
    assert!(coverage.values().map(|c| c.records).sum::<usize>() > 0);
    eprintln!("SAV-52 {} saves, {bytes} exact payload bytes", paths.len());
    for (tag, counts) in coverage {
        eprintln!("{tag}: {counts:?}");
    }
    eprintln!("versions/types/changeflags={variants:x?} shared_masks={masks:x?}");
    eprintln!("type/level/count/saved_chance_byte={fields:?}");
    eprintln!("type/resolved_target={targets:x?}");
    eprintln!("The saved chance-byte distribution is not a probability or padding definition.");
}

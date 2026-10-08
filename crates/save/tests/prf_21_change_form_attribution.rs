// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-21 checks exact change-form byte attribution over the user's saves.

use std::path::{Path, PathBuf};

use soe_save::Save;

fn collect(directory: &Path, saves: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, saves);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            saves.push(path);
        }
    }
}

#[test]
fn prf_21_change_form_bytes_reconcile_by_plugin_and_record_type_across_user_saves() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 save survey");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("SOE_TEST_SAVES contains no .ess files, skipping PRF-21 save survey");
        return;
    }

    let mut surveyed = 0;
    for path in paths {
        let save = Save::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let attributed: usize = save
            .change_form_sizes_by_plugin()
            .iter()
            .map(|row| row.bytes)
            .sum();
        let serialized: usize = save
            .change_forms
            .iter()
            .map(|form| {
                let mut bytes = Vec::new();
                form.write(&mut bytes);
                bytes.len()
            })
            .sum();
        assert_eq!(attributed, serialized, "{}", path.display());
        surveyed += 1;
    }
    assert!(surveyed > 0);
}

#[test]
fn prf_21_sections_plugins_and_papyrus_scripts_reconcile_across_user_saves() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 section survey");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let (save, profile) = Save::parse_profiled(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let plain = Save::parse(&bytes).unwrap();
        assert_eq!(
            save.body_bytes(),
            plain.body_bytes(),
            "profiling changed the parse of {}",
            path.display()
        );

        // The sections tile the body, so their sizes add up to its length.
        let body = save.body_bytes().len();
        let tiled: usize = profile
            .sections
            .iter()
            .filter(|section| section.name != "container")
            .map(|section| section.bytes)
            .sum();
        assert_eq!(tiled, body, "section sizes of {}", path.display());
        for name in [
            "container",
            "plugin_info",
            "location_table",
            "form_id_array",
            "table_1",
            "table_2",
            "change_forms",
            "table_3",
        ] {
            assert!(
                profile.sections.iter().any(|section| section.name == name),
                "{} has no {name} section",
                path.display()
            );
        }

        // Per-plugin splits add up to the whole they divide.
        assert_eq!(profile.change_form_nanos.len(), save.change_forms.len());
        let forms: usize = save
            .plugin_load_nanos(&profile)
            .iter()
            .map(|r| r.forms)
            .sum();
        assert_eq!(forms, save.change_forms.len(), "{}", path.display());
        let nanos: u64 = save
            .plugin_load_nanos(&profile)
            .iter()
            .map(|r| r.nanos)
            .sum();
        assert_eq!(nanos, profile.change_form_nanos.iter().sum::<u64>());
        let entries: usize = save
            .form_id_array_by_plugin()
            .iter()
            .map(|row| row.entries)
            .sum();
        assert_eq!(entries, save.form_id_array.len(), "{}", path.display());

        // Script instance data never exceeds the value data it is part of.
        if let Some(papyrus) = save.papyrus() {
            let papyrus = papyrus.unwrap();
            let sizes = save.papyrus_script_sizes(&papyrus).unwrap();
            let instances: usize = sizes.iter().map(|row| row.instances).sum();
            assert_eq!(instances, papyrus.instances.len(), "{}", path.display());
            let attributed: usize = sizes.iter().map(|row| row.bytes).sum();
            let instance_data: usize = papyrus
                .instance_data
                .iter()
                .map(|data| data.encoded_len().unwrap())
                .sum();
            assert_eq!(attributed, instance_data, "{}", path.display());
            assert!(
                attributed <= papyrus.execution_start - papyrus.data_start,
                "{}",
                path.display()
            );
        }
    }
}

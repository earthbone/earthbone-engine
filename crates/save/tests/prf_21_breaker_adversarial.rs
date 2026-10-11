// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_save::Save;

fn collect(root: &Path, saves: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
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
fn prf_21_breaker_profiled_ess_writes_preserve_and_partition_every_user_save() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 profiled ESS write survey");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    assert!(
        !paths.is_empty(),
        "the supplied save corpus contains ESS files"
    );

    for path in paths {
        let source = std::fs::read(&path).unwrap();
        let (save, parse_profile) = Save::parse_profiled(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let body_bytes = save.body_bytes().len();
        assert_eq!(
            parse_profile
                .sections
                .iter()
                .filter(|section| section.name != "container")
                .map(|section| section.bytes)
                .sum::<usize>(),
            body_bytes,
            "named ESS load sections must partition the body: {}",
            path.display()
        );
        let change_form_bytes: usize = save
            .change_forms
            .iter()
            .map(|form| {
                let mut bytes = Vec::new();
                form.write(&mut bytes);
                bytes.len()
            })
            .sum();
        assert_eq!(
            save.change_form_sizes_by_plugin()
                .iter()
                .map(|row| row.bytes)
                .sum::<usize>(),
            change_form_bytes,
            "per-plugin form bytes must reconcile: {}",
            path.display()
        );
        let plugin_loads = save.plugin_load_nanos(&parse_profile);
        assert_eq!(
            plugin_loads.iter().map(|row| row.forms).sum::<usize>(),
            save.change_forms.len(),
            "per-plugin load counts must reconcile: {}",
            path.display()
        );
        assert_eq!(
            plugin_loads.iter().map(|row| row.nanos).sum::<u64>(),
            parse_profile.change_form_nanos.iter().sum::<u64>(),
            "per-plugin load times must reconcile: {}",
            path.display()
        );
        if let Some(Ok(papyrus)) = save.papyrus() {
            let attributed: usize = save
                .papyrus_script_sizes(&papyrus)
                .unwrap()
                .iter()
                .map(|row| row.bytes)
                .sum();
            assert_eq!(
                attributed,
                papyrus
                    .instance_data
                    .iter()
                    .map(|data| data.encoded_len().unwrap())
                    .sum::<usize>(),
                "per-script Papyrus bytes must reconcile: {}",
                path.display()
            );
        }
        let (encoded, profile) = save.write_profiled().unwrap();
        assert_eq!(encoded, save.write().unwrap(), "{}", path.display());
        assert_eq!(
            profile
                .sections
                .iter()
                .map(|section| section.bytes)
                .sum::<usize>(),
            save.body_bytes().len(),
            "named ESS write sections must partition the body: {}",
            path.display()
        );
        assert_eq!(
            profile
                .sections
                .iter()
                .map(|section| section.name)
                .collect::<Vec<_>>(),
            [
                "plugin_info",
                "location_table",
                "table_1",
                "table_2",
                "change_forms",
                "table_3",
                "form_id_array",
                "visited_worldspaces",
                "unknown_3",
            ],
            "write profile section order follows the ESS body: {}",
            path.display()
        );
    }
}

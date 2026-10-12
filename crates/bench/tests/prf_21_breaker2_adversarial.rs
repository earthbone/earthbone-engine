// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

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
fn prf_21_cli_plugin_rows_match_saved_sections() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 CLI plugin report check");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    let Some(path) = paths.into_iter().next() else {
        eprintln!("SOE_TEST_SAVES contains no .ess files, skipping PRF-21 CLI plugin report check");
        return;
    };

    let source = std::fs::read(&path).unwrap();
    let (save, _) =
        Save::parse_profiled(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let expected_changes: BTreeMap<_, _> =
        save.change_form_sizes_by_plugin()
            .into_iter()
            .fold(BTreeMap::new(), |mut totals, row| {
                let total = totals.entry(row.plugin).or_insert((0, 0));
                total.0 += row.count;
                total.1 += row.bytes;
                totals
            });
    let expected_form_ids: BTreeMap<_, _> = save
        .form_id_array_by_plugin()
        .into_iter()
        .map(|row| (row.plugin, (row.entries, row.bytes)))
        .collect();

    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["save", path.to_str().unwrap()])
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();

    let reported_sections: usize = report
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.first().copied() == Some("save_section") && fields.get(1) != Some(&"container"))
                .then(|| fields.get(2)?.parse::<usize>().ok())
                .flatten()
        })
        .sum();
    let named_sections: BTreeMap<_, _> = report
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.first().copied() == Some("save_section"))
                .then(|| Some((fields.get(1)?.to_string(), fields.get(2)?.parse().ok()?)))
                .flatten()
        })
        .collect();
    let body_bytes = report
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.first().copied() == Some("save_section_bytes"))
                .then(|| fields.get(1)?.parse::<usize>().ok())
                .flatten()
        })
        .expect("report has save_section_bytes");
    assert_eq!(reported_sections, save.body_bytes().len());
    assert_eq!(body_bytes, save.body_bytes().len());

    let mut actual_changes = BTreeMap::new();
    let mut actual_form_ids = BTreeMap::new();
    for line in report.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().copied() != Some("save_plugin_section") || fields.len() < 5 {
            continue;
        }
        let entries = fields[2].parse::<usize>().unwrap();
        let bytes = fields[3].parse::<usize>().unwrap();
        let plugin = fields[4..].join(" ");
        match fields[1] {
            "change_forms" => {
                actual_changes.insert(plugin, (entries, bytes));
            }
            "form_id_array" => {
                actual_form_ids.insert(plugin, (entries, bytes));
            }
            _ => {}
        }
    }
    assert_eq!(actual_changes, expected_changes, "{}", path.display());
    assert_eq!(actual_form_ids, expected_form_ids, "{}", path.display());
    assert_eq!(
        actual_changes.values().map(|row| row.1).sum::<usize>(),
        named_sections["change_forms"]
    );
    assert_eq!(
        actual_form_ids.values().map(|row| row.1).sum::<usize>() + 4,
        named_sections["form_id_array"]
    );
    assert_eq!(
        actual_changes.values().map(|row| row.1).sum::<usize>(),
        save.change_forms
            .iter()
            .map(|form| {
                let mut encoded = Vec::new();
                form.write(&mut encoded);
                encoded.len()
            })
            .sum::<usize>()
    );
    assert_eq!(
        actual_form_ids.values().map(|row| row.1).sum::<usize>(),
        save.form_id_array.len() * 4
    );
}

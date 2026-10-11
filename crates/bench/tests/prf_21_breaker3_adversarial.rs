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

fn reported_plugin_loads(output: &str) -> BTreeMap<String, (usize, u64)> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.first().copied() != Some("save_plugin_load") || fields.len() < 4 {
                return None;
            }
            Some((
                fields[3..].join(" "),
                (fields[1].parse().ok()?, fields[2].parse().ok()?),
            ))
        })
        .collect()
}

#[test]
fn prf_21_cli_plugin_load_rows_match_parse_profile_on_repeated_runs() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 CLI plugin load check");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    let Some(path) = paths.into_iter().next() else {
        eprintln!("SOE_TEST_SAVES contains no ESS files, skipping PRF-21 CLI plugin load check");
        return;
    };

    let source = std::fs::read(&path).unwrap();
    let (save, profile) = Save::parse_profiled(&source)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let expected: BTreeMap<_, _> = save
        .plugin_load_nanos(&profile)
        .into_iter()
        .map(|row| (row.plugin, row.forms))
        .collect();
    let mut prior_counts = None;

    for _ in 0..2 {
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
        let actual = reported_plugin_loads(&report);
        assert_eq!(
            actual.iter().map(|row| row.1.0).sum::<usize>(),
            save.change_forms.len(),
            "plugin load counts must cover each decoded change form: {}",
            path.display()
        );
        assert_eq!(
            actual
                .iter()
                .map(|(plugin, row)| (plugin.clone(), row.0))
                .collect::<BTreeMap<_, _>>(),
            expected,
            "per-plugin counts must match parsed change forms: {}",
            path.display()
        );
        let reported_total = report
            .lines()
            .find_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.first().copied() == Some("save_plugin_load_ns"))
                    .then(|| fields.get(1)?.parse::<u64>().ok())
                    .flatten()
            })
            .expect("report includes total plugin load time");
        assert_eq!(
            reported_total,
            actual.values().map(|row| row.1).sum::<u64>(),
            "reported plugin load time must equal its rows: {}",
            path.display()
        );
        let counts = actual
            .iter()
            .map(|(plugin, row)| (plugin.clone(), row.0))
            .collect::<BTreeMap<_, _>>();
        assert!(
            prior_counts.replace(counts.clone()).is_none_or(|prior| prior == counts),
            "repeated reports must preserve plugin counts: {}",
            path.display()
        );
    }
}

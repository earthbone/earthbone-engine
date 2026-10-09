// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-21 checks the user-facing save report against a real ESS file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use soe_save::{RefKind, Save};

struct Report {
    output: Output,
}

static REPORT: OnceLock<Option<Report>> = OnceLock::new();

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

fn user_saves() -> Vec<PathBuf> {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || {
            PathBuf::from(
                "/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves",
            )
        },
        PathBuf::from,
    );
    let mut saves = Vec::new();
    collect(&root, &mut saves);
    saves.sort();
    saves
}

fn report() -> Option<&'static Report> {
    REPORT
        .get_or_init(|| {
            let Some(path) = user_saves().into_iter().next() else {
                eprintln!("no user saves found, set SOE_TEST_SAVES, skipping PRF-21 CLI checks");
                return None;
            };
            let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
                .args(["save", path.to_str().expect("save path is UTF-8")])
                .env_remove("SKYRIM_DATA")
                .output()
                .expect("soe-bench starts");
            Some(Report { output })
        })
        .as_ref()
}

fn stdout() -> &'static str {
    std::str::from_utf8(&report().expect("user save exists").output.stdout)
        .expect("soe-bench output is UTF-8")
}

#[test]
fn prf_21_plugin_type_rows_match_each_form_and_reconcile_per_save() {
    let paths = user_saves();
    if paths.is_empty() {
        eprintln!("no user saves found, set SOE_TEST_SAVES, skipping PRF-21 attribution check");
        return;
    }

    for path in paths {
        let save = Save::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut expected = BTreeMap::<(String, &'static str), (usize, usize)>::new();
        for form in &save.change_forms {
            let form_id = save.resolve(form.form);
            let plugin = form_id
                .and_then(|id| {
                    if id >> 24 == 0xFE {
                        save.light_plugins.get(((id >> 12) & 0xFFF) as usize)
                    } else {
                        save.plugins.get((id >> 24) as usize)
                    }
                })
                .map_or_else(
                    || match form.form.kind() {
                        RefKind::Created => "<created>".to_owned(),
                        RefKind::Invalid => "<invalid>".to_owned(),
                        RefKind::Index => "<unresolved>".to_owned(),
                        RefKind::Default => "<unknown-plugin>".to_owned(),
                    },
                    |name| name.to_text(),
                );
            let mut bytes = Vec::new();
            form.write(&mut bytes);
            let total = expected.entry((plugin, form.record_type())).or_default();
            total.0 += 1;
            total.1 += bytes.len();
        }

        let actual: BTreeMap<_, _> = save
            .change_form_sizes_by_plugin()
            .into_iter()
            .map(|row| ((row.plugin, row.record_type), (row.count, row.bytes)))
            .collect();
        assert_eq!(actual, expected, "{}", path.display());
    }
}

#[test]
fn prf_21_save_command_reports_supported_sizes_and_parse_timing() {
    let Some(report) = report() else {
        return;
    };
    assert!(
        report.output.status.success(),
        "{}",
        String::from_utf8_lossy(&report.output.stderr)
    );
    for field in [
        "save_parse_median ",
        "save_file_bytes ",
        "save_table_1_bytes ",
        "save_table_2_bytes ",
        "save_table_3_bytes ",
        "save_global_section_",
        "save_plugin_change ",
        "save_papyrus_bytes ",
        "save_papyrus_scripts ",
        "save_papyrus_instances ",
    ] {
        assert!(
            stdout().lines().any(|line| line.starts_with(field)),
            "save report has no {field} rows"
        );
    }
}

#[test]
fn prf_21_save_command_reports_each_named_section_size() {
    if report().is_none() {
        return;
    }
    assert!(
        report().unwrap().output.status.success(),
        "{}",
        String::from_utf8_lossy(&report().unwrap().output.stderr)
    );
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_section ")),
        "save report has no named section size rows"
    );
}

#[test]
fn prf_21_save_command_reports_load_time_per_section() {
    if report().is_none() {
        return;
    }
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_section_time ")),
        "save report has no load timings per section"
    );
}

#[test]
fn prf_21_save_command_reports_plugin_sizes() {
    if report().is_none() {
        return;
    }
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_plugin_section ")),
        "save report has no per-plugin section sizes"
    );
}

#[test]
fn prf_21_save_command_reports_load_time_per_plugin() {
    if report().is_none() {
        return;
    }
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_plugin_load ")),
        "save report has no per-plugin load timings"
    );
}

#[test]
fn prf_21_save_command_attributes_papyrus_bytes_to_script_and_plugin() {
    if report().is_none() {
        return;
    }
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_papyrus_script ")),
        "save report has no Papyrus script rows"
    );
    assert!(
        stdout()
            .lines()
            .filter(|line| line.starts_with("save_papyrus_script "))
            .all(|line| line.split_whitespace().count() >= 4),
        "Papyrus script rows must include script identity and owning plugin"
    );
}

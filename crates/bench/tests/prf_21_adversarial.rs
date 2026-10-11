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
fn prf_21_save_command_reports_write_time_per_section() {
    if report().is_none() {
        return;
    }
    assert!(
        stdout()
            .lines()
            .any(|line| line.starts_with("save_section_save_ns ")),
        "save report has no section write timings"
    );
}

#[test]
fn prf_21_save_write_sections_cover_ess_body_and_preserve_encoded_bytes() {
    let Some(path) = user_saves().into_iter().next() else {
        eprintln!("no user saves found, skipping PRF-21 ESS writer profile check");
        return;
    };
    let save = Save::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (profiled_bytes, profile) = save.write_profiled().unwrap();
    assert_eq!(profiled_bytes, save.write().unwrap());
    assert_eq!(
        profile
            .sections
            .iter()
            .map(|section| section.bytes)
            .sum::<usize>(),
        save.body_bytes().len()
    );
    for section in [
        "plugin_info",
        "location_table",
        "table_1",
        "table_2",
        "change_forms",
        "table_3",
        "form_id_array",
        "visited_worldspaces",
        "unknown_3",
    ] {
        assert!(
            profile.sections.iter().any(|row| row.name == section),
            "write profile omits {section}"
        );
    }
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

#[test]
fn prf_21_save_command_papyrus_plugin_totals_match_script_owners() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 Papyrus ownership check");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();

    let (path, expected_scripts) = paths
        .into_iter()
        .find_map(|path| {
            let save =
                Save::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let papyrus = save.papyrus()?.ok()?;
            let rows = save.papyrus_script_sizes(&papyrus).unwrap();
            (!rows.is_empty()).then_some((path, rows))
        })
        .expect("the supplied save corpus contains Papyrus script instances");

    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["save", path.to_str().expect("save path is UTF-8")])
        .env_remove("SKYRIM_DATA")
        .output()
        .expect("soe-bench starts");
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("soe-bench output is UTF-8");

    let expected_script_rows: BTreeMap<_, _> = expected_scripts
        .iter()
        .map(|row| {
            (
                (row.plugin.clone(), row.script.clone()),
                (row.instances, row.bytes),
            )
        })
        .collect();
    let actual_script_rows: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.first().copied() != Some("save_papyrus_script") || fields.len() < 5 {
                return None;
            }
            let instances = fields[2].parse().ok()?;
            let bytes = fields[3].parse().ok()?;
            Some((
                (fields[4..].join(" "), fields[1].to_owned()),
                (instances, bytes),
            ))
        })
        .collect();
    assert_eq!(
        actual_script_rows,
        expected_script_rows,
        "{}",
        path.display()
    );

    let mut expected_plugins = BTreeMap::<String, (usize, usize)>::new();
    for row in &expected_scripts {
        let totals = expected_plugins.entry(row.plugin.clone()).or_default();
        totals.0 += row.instances;
        totals.1 += row.bytes;
    }
    let actual_plugins: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.first().copied() != Some("save_papyrus_script_plugin") || fields.len() < 4 {
                return None;
            }
            Some((
                fields[3..].join(" "),
                (fields[1].parse().ok()?, fields[2].parse().ok()?),
            ))
        })
        .collect();
    assert_eq!(actual_plugins, expected_plugins, "{}", path.display());
}

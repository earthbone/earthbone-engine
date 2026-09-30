// SPDX-License-Identifier: GPL-3.0-or-later

//! Command-line support for replaying validation scenarios from an MO2 profile.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::harness::{self, Check, Report, Scenario, Step};
use crate::Game;
use soe_bsa::Vfs;
use soe_profile::mo2::Mo2Instance;

/// One executed scenario and the oracle captures it cites.
#[derive(Debug)]
pub struct ScenarioResult {
    pub name: String,
    pub claim: String,
    pub captures: Vec<PathBuf>,
    pub report: Report,
}

/// Resolve a portable MO2 profile and run every `.scenario` file below a directory.
pub fn run_profile(
    game_dir: &Path,
    mo2_root: &Path,
    profile_name: &str,
    scenarios_dir: &Path,
) -> Result<Vec<ScenarioResult>, String> {
    let input = Mo2Instance::portable(mo2_root, game_dir, profile_name)
        .loadout_input()
        .map_err(|e| e.to_string())?;
    let loadout = input.resolve().map_err(|e| e.to_string())?;
    let soe_profile::Loadout {
        data_roots,
        load_order: profile_order,
        archives,
        ..
    } = loadout;
    let mut order = soe_esm::LoadOrder::new();
    for entry in profile_order.active() {
        let plugin = soe_esm::read_plugin(&entry.path)
            .map_err(|e| format!("{}: {e}", entry.path.display()))?
            .map_err(|e| format!("{}: {e}", entry.path.display()))?;
        order
            .add(&entry.name, plugin)
            .map_err(|e| format!("{}: {e}", entry.path.display()))?;
    }
    let order = Arc::new(order);

    let mut vfs = Vfs::new();
    for archive in &archives {
        if let Some(path) = &archive.path {
            vfs.add_archive_path(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    for root in &data_roots {
        if root.is_dir() {
            vfs.add_loose_root(root)
                .map_err(|e| format!("{}: {e}", root.display()))?;
        }
    }
    let mut paths = scenario_paths(scenarios_dir)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "no .scenario files found under {}",
            scenarios_dir.display()
        ));
    }

    paths
        .into_iter()
        .map(|path| {
            let mut game = Game::from_shared_order(Arc::clone(&order));
            game.load_scripts_from_vfs(&vfs);
            game.apply_mod_data(&data_roots);
            game.start();
            run_scenario(&mut game, &path)
        })
        .collect()
}

/// Render scenario outcomes and the captures each scenario cites.
pub fn format_report(results: &[ScenarioResult]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Conformance report: {} scenario(s)", results.len());
    for result in results {
        let state = if result.report.passed() {
            "PASS"
        } else {
            "FAIL"
        };
        let _ = writeln!(out, "{state} {} [{}]", result.name, result.claim);
        let _ = writeln!(
            out,
            "  steps: {}, checks: {}",
            result.report.steps, result.report.checks
        );
        if result.captures.is_empty() {
            let _ = writeln!(out, "  captures: none");
        } else {
            let _ = writeln!(out, "  captures:");
            for capture in &result.captures {
                let _ = writeln!(out, "    {}", capture.display());
            }
        }
        for failure in &result.report.failures {
            let _ = writeln!(out, "  {failure}");
        }
    }
    let outcome = if results.iter().all(|result| result.report.passed()) {
        "PASS"
    } else {
        "FAIL"
    };
    let _ = writeln!(out, "Overall: {outcome}");
    out
}

fn scenario_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err(format!(
            "scenario directory does not exist: {}",
            root.display()
        ));
    }
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "scenario")
            {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

fn run_scenario(game: &mut Game, path: &Path) -> Result<ScenarioResult, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let claim = claim_line(&text).ok_or_else(|| {
        format!(
            "{}: first comment line must be `# preserve: ...` or `# redesign: ...`",
            path.display()
        )
    })?;
    let scenario_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut scenario = Scenario::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut captures = Vec::new();
    for (_, step) in &mut scenario.steps {
        let Step::Expect(Check::TracesMatch(capture)) = step else {
            continue;
        };
        let resolved = if Path::new(capture).is_absolute() {
            PathBuf::from(capture.as_str())
        } else {
            scenario_dir.join(capture.as_str())
        };
        captures.push(resolved.clone());
        *capture = resolved.to_string_lossy().into_owned();
    }
    captures.sort();
    captures.dedup();
    let name = path
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok(ScenarioResult {
        name,
        claim,
        captures,
        report: harness::run(game, &scenario),
    })
}

fn claim_line(text: &str) -> Option<String> {
    let first_comment = text
        .lines()
        .find(|line| line.trim_start().starts_with('#'))?;
    let claim = first_comment.trim().trim_start_matches('#').trim();
    ["preserve:", "redesign:"]
        .iter()
        .find(|prefix| claim.starts_with(**prefix))?;
    Some(claim.to_string())
}

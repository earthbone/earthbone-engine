// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn run_python(assertions: &str) {
    let module = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/performance.py");
    let script = format!(
        "import importlib.util\nfrom pathlib import Path\nspec = importlib.util.spec_from_file_location('performance', r'{}')\nperf = importlib.util.module_from_spec(spec)\nspec.loader.exec_module(perf)\n{assertions}",
        module.display()
    );
    let output = Command::new("python3")
        .args(["-c", &script])
        .output()
        .expect("run texture sweep contract check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn prf_03_non_power_of_two_cap_is_the_last_completed_rung() {
    run_python(
        "visited = []\ndef measure(load):\n    visited.append(load)\n    return 'pass'\nresult = perf.texture_capacity_sweep(measure, 3, 10, 2, {'resolution': '1600x900'})\nassert visited == [3, 6, 10], visited\nassert result['status'] == 'limit_not_found', result\nassert result['capacity_textures'] == 10, result\nassert result['conditions'] == {'resolution': '1600x900', 'repeats_per_rung': 2}, result",
    );
}

#[test]
fn prf_03_later_pass_cannot_raise_capacity_past_first_unstable_rung() {
    run_python(
        "visited = []\ndef measure(load):\n    visited.append(load)\n    return {2: 'pass', 4: 'unstable', 8: 'pass'}[load]\nresult = perf.texture_capacity_sweep(measure, 2, 8, 3, {'seed': 17})\nassert visited == [2, 4, 8], visited\nassert result['status'] == 'unstable', result\nassert result['capacity_textures'] == 2, result\nassert result['first_nonpass'] == {'load': 4, 'status': 'unstable'}, result",
    );
}

#[test]
fn prf_03_error_rung_does_not_claim_capacity_and_sweep_finishes() {
    run_python(
        "visited = []\ndef measure(load):\n    visited.append(load)\n    return 'error' if load == 1 else 'pass'\nresult = perf.texture_capacity_sweep(measure, 1, 4, 5, {'max_video_mib': 4096})\nassert visited == [1, 2, 4], visited\nassert result['status'] == 'error', result\nassert result['capacity_textures'] == 0, result\nassert result['conditions']['repeats_per_rung'] == 5, result",
    );
}

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
        .expect("run soak assessor contract check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn prf_04_each_soak_window_requires_frame_time_fields() {
    run_python(
        "import tempfile\nfrom pathlib import Path\nrow = {'samples': 1, 'p50_ms': 1.0, 'p999_ms': 1.0, 'worst_ms': 1.0}\nwith tempfile.TemporaryDirectory() as directory:\n    path = Path(directory) / 'samples.log'\n    path.write_text('SOE_SAMPLE ' + __import__('json').dumps(row))\n    try:\n        perf.read_samples(path)\n    except ValueError as error:\n        assert 'p99_ms' in str(error), error\n    else:\n        raise AssertionError('missing frame-time telemetry was accepted')",
    );
}

#[test]
fn prf_04_one_missing_rss_window_cannot_pass_a_soak() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=200, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=100.0, elapsed_s=100.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=None, elapsed_s=200.0)]\nreasons, invalid = perf.assess(samples, args)\nassert reasons == ['soak needs at least two windows with RSS observations'], reasons\nassert invalid is True",
    );
}

#[test]
fn prf_04_non_finite_frame_samples_cannot_enter_a_soak_report() {
    run_python(
        "import tempfile\nfrom pathlib import Path\nrow = {'samples': 1, 'p50_ms': 1.0, 'p99_ms': float('nan'), 'p999_ms': 1.0, 'worst_ms': 1.0}\nwith tempfile.TemporaryDirectory() as directory:\n    path = Path(directory) / 'samples.log'\n    path.write_text('SOE_SAMPLE ' + __import__('json').dumps(row))\n    try:\n        perf.read_samples(path)\n    except ValueError as error:\n        assert 'p99_ms' in str(error), error\n    else:\n        raise AssertionError('non-finite frame-time telemetry was accepted')",
    );
}

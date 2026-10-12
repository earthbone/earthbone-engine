// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn run_python(assertions: &str) {
    let module = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/performance.py");
    let script = format!(
        "import importlib.util\nspec = importlib.util.spec_from_file_location('performance', r'{}')\nperf = importlib.util.module_from_spec(spec)\nspec.loader.exec_module(perf)\n{assertions}",
        module.display()
    );
    let output = Command::new("python3")
        .args(["-c", &script])
        .output()
        .expect("run soak runner contract check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn prf_04_requested_24_hour_duration_reaches_the_cpu_child() {
    run_python(
        "from pathlib import Path\nfrom types import SimpleNamespace\nargs = SimpleNamespace(backend='cpu', binary=Path('/tmp/soe-bench'), scenario='mixed', samples=120, warmup=30, seed=1, duration=86400, vm_budget=100000, godot=None)\nargv, overrides = perf.command(args, 8)\nassert overrides == {}\nassert argv[argv.index('--seconds') + 1] == '86400', argv",
    );
}

#[test]
fn prf_04_soak_rejects_a_duration_sample_that_rewinds_after_finishing() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=86400, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=1, rss_mib=100.0, elapsed_s=86400.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=1, rss_mib=100.0, elapsed_s=86399.0)]\nreasons, invalid = perf.assess(samples, args)\nassert reasons == ['soak windows need increasing elapsed-time telemetry'], reasons\nassert invalid is True",
    );
}

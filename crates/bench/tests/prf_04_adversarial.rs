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
fn prf_04_exact_24_hour_soak_requires_all_windows_and_rss_telemetry() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=86400, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=100.0, elapsed_s=86399.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=101.0, elapsed_s=86400.0)]\nassert perf.assess(samples, args) == ([], False)",
    );
}

#[test]
fn prf_04_soak_one_second_short_is_not_a_pass() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=86400, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=100.0, elapsed_s=86398.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=101.0, elapsed_s=86399.0)]\nreasons, invalid = perf.assess(samples, args)\nassert reasons == ['soak ended before requested duration'], reasons\nassert invalid is True",
    );
}

#[test]
fn prf_04_soak_within_rss_growth_budget_has_no_leak_verdict() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=200, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=100.0, elapsed_s=100.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=164.0, elapsed_s=200.0)]\nassert perf.assess(samples, args) == ([], False)",
    );
}

#[test]
fn prf_04_soak_over_rss_growth_budget_is_reported_for_investigation() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=86400, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=100.0, elapsed_s=100.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=164.01, elapsed_s=200.0)]\nreasons, invalid = perf.assess(samples, args)\nassert reasons == ['RSS growth 64.010 MiB exceeds 64.0'], reasons\nassert invalid is False",
    );
}

#[test]
fn prf_04_soak_without_rss_measurements_cannot_pass() {
    run_python(
        "from types import SimpleNamespace\nargs = SimpleNamespace(mode='soak', backend='cpu', duration=86400, p99_ms=16.7, worst_ms=33.0, max_backlog=0, max_video_mib=4096.0, max_allocations=100000, max_growth_mib=64.0)\nsamples = [dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=None, elapsed_s=100.0), dict(p99_ms=1.0, worst_ms=2.0, backlog_peak=0, video_mib=0.0, peak_allocations=1, peak_allocation_bytes=10, rss_mib=None, elapsed_s=200.0)]\nreasons, invalid = perf.assess(samples, args)\nassert reasons == ['soak needs at least two windows with RSS observations'], reasons\nassert invalid is True",
    );
}

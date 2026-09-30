"""Regression tests for capacity reports, gates and failed child processes."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

MODULE = Path(__file__).resolve().parents[1] / "performance.py"
SPEC = importlib.util.spec_from_file_location("performance", MODULE)
perf = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(perf)


def arguments(**overrides):
    values = dict(mode="check", backend="cpu", scenario="scripts", binary=Path(sys.executable),
                  samples=120, warmup=10, seed=1, vm_budget=100000, p99_ms=16.7, worst_ms=33,
                  max_backlog=0, max_video_mib=4096, max_rss_mib=2048, max_growth_mib=64, timeout=2, duration=10)
    values.update(overrides)
    return SimpleNamespace(**values)


def sample(**overrides):
    values = dict(samples=120, p50_ms=1, p99_ms=2, p999_ms=3, worst_ms=3, backlog_peak=0,
                  rss_mib=32, elapsed_s=10)
    values.update(overrides)
    return values


class CapacityTests(unittest.TestCase):
    def test_sweep_repeats_boundary_measurement_and_refines_bracket(self):
        visited = []

        def measure(load):
            visited.append(load)
            return "pass" if load < 13 else "breach"

        result = perf.sweep(measure, 1, 64, 8)
        self.assertEqual(result, dict(status="boundary_found", last_pass=12, first_breach=13))
        self.assertIn(16, visited)
        self.assertNotIn(32, visited)

    def test_cap_without_failure_does_not_claim_an_engine_limit(self):
        self.assertEqual(perf.sweep(lambda _: "pass", 2, 9, 4),
                         dict(status="limit_not_found", last_pass=9, tested_maximum=9))

    def test_first_rung_failure_has_no_invented_passing_bound(self):
        self.assertIsNone(perf.sweep(lambda _: "breach", 8, 64, 4)["last_pass"])

    def test_noise_and_harness_errors_do_not_become_capacity_limits(self):
        self.assertEqual(perf.classify([{"status": "pass"}, {"status": "breach"}]), "unstable")
        self.assertEqual(perf.classify([{"status": "error"}, {"status": "breach"}]), "error")
        self.assertEqual(perf.sweep(lambda _: "error", 1, 64, 4)["status"], "error")

    def test_fast_tick_with_backlog_fails(self):
        reasons, invalid = perf.assess([sample(backlog_peak=1)], arguments())
        self.assertFalse(invalid)
        self.assertIn("backlog_peak", reasons[0])

    def test_soak_checks_duration_memory_growth_and_missing_measurements(self):
        args = arguments(mode="soak")
        reasons, invalid = perf.assess([sample(elapsed_s=2), sample(rss_mib=120)], args)
        self.assertFalse(invalid)
        self.assertIn("RSS growth", reasons[0])
        self.assertTrue(perf.assess([sample(elapsed_s=2)], args)[1])
        self.assertTrue(perf.assess([sample(rss_mib=None), sample()], args)[1])
        self.assertTrue(perf.assess([sample(), sample(elapsed_s=9)], args)[1])

    def test_nan_empty_and_inconsistent_samples_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "child.log"
            for row in [sample(p99_ms=float("nan")), sample(samples=0), sample(p50_ms=50), sample(rss_mib=float("inf")), sample(backlog_peak=-1)]:
                path.write_text("SOE_SAMPLE " + json.dumps(row) + "\n")
                with self.assertRaises(ValueError):
                    perf.read_samples(path)
        self.assertTrue(perf.assess([], arguments())[1])

    def test_child_crash_is_error_and_timeout_is_not_a_measured_cliff(self):
        old = perf.command
        try:
            with tempfile.TemporaryDirectory() as directory:
                args = arguments(out=Path(directory), timeout=0.1)
                for code, reason in [("raise SystemExit(7)", "child exited 7"), ("import time; time.sleep(10)", "timeout")]:
                    perf.command = lambda *_: ([sys.executable, "-c", code], {})
                    run = perf.run_once(args, 1, 0)
                    self.assertEqual(run["status"], "error")
                    self.assertIn(reason, run["reasons"])
        finally:
            perf.command = old

    def test_script_errors_and_empty_rendering_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "child.log"
            path.write_text("SCRIPT ERROR: initialization failed\nSOE_SAMPLE " + json.dumps(sample()) + "\n")
            with self.assertRaises(ValueError):
                perf.read_samples(path)
        self.assertTrue(perf.assess([sample(draw_calls=0, primitives=0)], arguments(backend="godot"))[1])

    def test_budget_breach_can_end_soak_early_without_becoming_harness_error(self):
        reasons, invalid = perf.assess([sample(p99_ms=40, worst_ms=40, elapsed_s=2)], arguments(mode="soak"))
        self.assertFalse(invalid)
        self.assertTrue(reasons)

    def test_zero_exit_without_metrics_does_not_pass(self):
        old = perf.command
        try:
            with tempfile.TemporaryDirectory() as directory:
                perf.command = lambda *_: ([sys.executable, "-c", "print('RESULT: PASS')"], {})
                run = perf.run_once(arguments(out=Path(directory)), 1, 0)
                self.assertEqual(run["status"], "error")
        finally:
            perf.command = old

    def test_prf_04_full_day_soak_records_memory_and_frame_time(self):
        args = arguments(mode="soak", duration=86400)
        rows = [sample(elapsed_s=86390, rss_mib=128),
                sample(elapsed_s=86400, rss_mib=150, p99_ms=8, worst_ms=20)]

        reasons, invalid = perf.assess(rows, args)

        self.assertEqual(reasons, [])
        self.assertFalse(invalid)

    def test_prf_04_soak_requires_all_memory_windows_and_stays_under_growth_budget(self):
        args = arguments(mode="soak", duration=86400)
        rows = [sample(elapsed_s=300, rss_mib=128), sample(elapsed_s=600, rss_mib=None)]

        reasons, invalid = perf.assess(rows, args)

        self.assertTrue(invalid)
        self.assertIn("RSS observations", reasons[0])

        rows = [sample(elapsed_s=300, rss_mib=128), sample(elapsed_s=600, rss_mib=193)]
        reasons, invalid = perf.assess(rows, args)

        self.assertFalse(invalid)
        self.assertIn("RSS growth", reasons[0])

    def test_prf_04_soak_requires_frame_times_under_both_budgets(self):
        args = arguments(mode="soak", duration=600)
        rows = [sample(elapsed_s=300, p99_ms=17, worst_ms=20),
                sample(elapsed_s=600, p99_ms=10, worst_ms=34)]

        reasons, invalid = perf.assess(rows, args)

        self.assertFalse(invalid)
        self.assertIn("p99_ms", reasons[0])
        self.assertIn("worst_ms", reasons[1])

    def test_prf_04_soak_requires_monotonic_elapsed_time_for_full_duration(self):
        args = arguments(mode="soak", duration=86400)
        rows = [sample(elapsed_s=500, rss_mib=128), sample(elapsed_s=None, rss_mib=129)]

        reasons, invalid = perf.assess(rows, args)

        self.assertTrue(invalid)
        self.assertIn("elapsed-time telemetry", reasons[0])

        rows = [sample(elapsed_s=500, rss_mib=128), sample(elapsed_s=499, rss_mib=129)]

        reasons, invalid = perf.assess(rows, args)

        self.assertTrue(invalid)
        self.assertIn("increasing elapsed-time", reasons[0])

        rows = [sample(elapsed_s=86399, rss_mib=128), sample(elapsed_s=86399, rss_mib=129)]
        reasons, invalid = perf.assess(rows, args)

        self.assertTrue(invalid)
        self.assertIn("increasing elapsed-time", reasons[0])

        rows = [sample(elapsed_s=86390, rss_mib=128), sample(elapsed_s=86399, rss_mib=129)]
        reasons, invalid = perf.assess(rows, args)

        self.assertTrue(invalid)
        self.assertIn("requested duration", reasons[0])


if __name__ == "__main__":
    unittest.main()

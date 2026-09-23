#!/usr/bin/env python3
"""Adversarial tests for tools/performance.py: PRF-02's benchmark runner.

Hermetic, stdlib only, no SKYRIM_DATA and no Godot binary needed. These
attack the tool's own false-pass surface: a broken or missing measurement
must fail loudly, never read as a quiet "within budgets". Sourced against
docs/performance-limits.md's measurement contract and the PRF-02 done
check ("Whiterun exterior block reports p50, p99, p99.9, worst hitch 16.7
ms bar").

Run:
    python3 tools/performance_adversarial.py -v
"""

import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("performance", ROOT / "tools" / "performance.py")
assert SPEC is not None and SPEC.loader is not None, "could not load tools/performance.py"
performance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(performance)


def sample(**overrides):
    row = {
        "samples": 600,
        "p50_ms": 5.0,
        "p99_ms": 10.0,
        "p999_ms": 12.0,
        "worst_ms": 15.0,
        # assess()'s godot branch fails the whole run when either is <= 0
        # (docs/performance-limits.md: "Script errors and zero rendered
        # work fail even after an exit code of zero"), so a fixture for
        # the worst_ms budget itself must clear that gate first or every
        # such sample reads as "invalid", not "breach".
        "draw_calls": 100,
        "primitives": 1000,
    }
    row.update(overrides)
    return row


def write_log(tmp_path, lines):
    log = tmp_path / "run.log"
    log.write_text("\n".join(lines) + "\n")
    return log


class Args:
    """A stand-in for argparse.Namespace with performance.py's defaults."""

    def __init__(self, **overrides):
        self.backend = "godot"
        self.mode = "check"
        self.p99_ms = 16.7
        self.worst_ms = 33.0
        self.max_backlog = 0
        self.max_video_mib = 4096.0
        self.max_growth_mib = 64.0
        for key, value in overrides.items():
            setattr(self, key, value)


class RejectsBrokenSamples(unittest.TestCase):
    """A broken SOE_SAMPLE line must raise, not be silently accepted.

    Source: read_samples's own field checks (tools/performance.py) name
    exactly this contract. These tests confirm the guards actually fire
    on the inputs a corrupted or half-written child process would emit,
    rather than trusting the code reading matches the code as written.
    """

    def test_boolean_percentile_is_rejected_not_truthy_accepted(self):
        # JSON's true/false decode to Python bool, and bool is an int
        # subclass, so a naive `isinstance(value, (int, float))` check
        # alone would silently accept `"p99_ms": true` as 1.0 millisecond,
        # the fastest possible false pass. Confirm the explicit bool guard
        # actually rejects it instead of relying on it being there in the
        # source.
        row = sample(p99_ms=True)
        log = write_log(Path(self._tmp()), [f"SOE_SAMPLE {json.dumps(row)}"])
        with self.assertRaisesRegex(ValueError, "p99_ms"):
            performance.read_samples(log)

    def test_zero_sample_window_is_rejected(self):
        # "samples": 0 means the window closed without ever observing a
        # frame. The report() call in bench_metrics.gd cannot itself
        # produce this (ready() requires frame_ms.size() >= 600 first),
        # but a hand-crafted or truncated log must still be refused, since
        # this is the exact shape a scenario that quit before its first
        # window would take.
        row = sample(samples=0)
        log = write_log(Path(self._tmp()), [f"SOE_SAMPLE {json.dumps(row)}"])
        with self.assertRaisesRegex(ValueError, "empty sample window"):
            performance.read_samples(log)

    _counter = 0

    def _tmp(self):
        import tempfile

        RejectsBrokenSamples._counter += 1
        directory = tempfile.mkdtemp(prefix=f"perf-adversarial-{RejectsBrokenSamples._counter}-")
        return directory


class FalsePassOnLooseDefaultBudget(unittest.TestCase):
    """The PRF-02 done check names one bar for the whole family of
    percentiles: "p50, p99, p99.9, worst hitch 16.7 ms". performance.py's
    own --worst-ms default is 33.0 ms (docs/performance-limits.md:
    "p99 at most 16.7 ms, worst at most 33 ms"), twice that bar. Anyone
    who runs `tools/performance.py check --backend godot --scenario
    exterior` without explicitly overriding `--worst-ms 16.7` gets a tool
    that calls a 20 ms worst frame a pass, while the task this tool exists
    to gate calls the same frame a failure. That gap between the tool's
    default and the project's actual acceptance bar is the false pass:
    the tool's exit code and printed status would read "pass" for a
    result the done check rejects.
    """

    def test_default_worst_budget_passes_a_frame_prf02_would_reject(self):
        args = Args()  # worst_ms defaults to 33.0, not the task's 16.7 bar
        samples = [sample(p99_ms=12.0, worst_ms=20.0)]
        reasons, invalid = performance.assess(samples, args)
        self.assertFalse(invalid)
        self.assertEqual(
            reasons,
            [],
            "the default --worst-ms budget (33.0 ms) let a 20.0 ms worst "
            "frame through with no reasons at all, which is a silent pass "
            "against the tool's own defaults for a frame that fails the "
            "PRF-02 done check's 16.7 ms bar on worst; the runner must be "
            "invoked with --worst-ms 16.7 (and --p99-ms 16.7, already the "
            "default) or its 'pass' does not mean what the task means",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)

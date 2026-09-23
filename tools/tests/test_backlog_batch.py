"""Regression tests for backlog row routing and readiness."""

import importlib.util
from pathlib import Path
import unittest

MODULE = Path(__file__).resolve().parents[1] / "backlog_batch.py"
SPEC = importlib.util.spec_from_file_location("backlog_batch", MODULE)
assert SPEC is not None and SPEC.loader is not None
batch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(batch)


def row(row_id, area, crates, status="open", depends_on="-", done_check="Planned check"):
    return {"id": row_id, "area": area, "crates": crates, "status": status,
            "depends_on": depends_on, "done_check": done_check}


class RoutingTests(unittest.TestCase):
    def test_area_rules_win_before_crate_rules(self):
        self.assertEqual(batch.task_type(row("SCR-01", "SCR", "game,godot-host")), "native")
        self.assertEqual(batch.task_type(row("SHM-03", "SHM", "commonlib-shim,game")), "shim")
        self.assertEqual(batch.task_type(row("TST-01", "TST", "godot-host,game")), "tooling")

    def test_crates_pick_host_then_gameplay_then_format(self):
        self.assertEqual(batch.task_type(row("RND-01", "RND", "nif,godot-host")), "host")
        self.assertEqual(batch.task_type(row("SAV-52", "SAV", "game,save")), "gameplay")
        self.assertEqual(batch.task_type(row("ANM-24", "ANM", "hkx")), "format")


class ReadinessTests(unittest.TestCase):
    def test_only_open_rows_with_finished_dependencies_are_ready(self):
        rows = [
            row("A-01", "CMB", "game", status="done"),
            row("A-02", "CMB", "game", depends_on="A-01"),
            row("A-03", "CMB", "game", depends_on="A-01,A-02"),
            row("A-04", "CMB", "game", status="blocked"),
            row("A-05", "CMB", "game", status="wip"),
        ]
        self.assertEqual([r["id"] for r in batch.ready_rows(rows)], ["A-02"])

    def test_original_runtime_checks_are_flagged(self):
        self.assertTrue(batch.needs_runtime(row("PRG-21", "PRG", "game",
                                                done_check="Authored isolated original-runtime traces")))
        self.assertFalse(batch.needs_runtime(row("CMB-10", "CMB", "game")))


if __name__ == "__main__":
    unittest.main()

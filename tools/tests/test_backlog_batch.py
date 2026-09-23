"""Regression tests for backlog row routing and readiness."""

import importlib.util
from pathlib import Path
import unittest

MODULE = Path(__file__).resolve().parents[1] / "backlog_batch.py"
SPEC = importlib.util.spec_from_file_location("backlog_batch", MODULE)
assert SPEC is not None and SPEC.loader is not None
batch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(batch)


def row(row_id, area, crates, status="open", depends_on="-", done_check="Planned check",
        difficulty="high", title="Title"):
    return {"id": row_id, "area": area, "crates": crates, "status": status, "title": title,
            "depends_on": depends_on, "done_check": done_check, "difficulty": difficulty}


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

    def test_checks_only_the_user_can_run_are_flagged(self):
        self.assertTrue(batch.needs_user(row("PRG-21", "PRG", "game",
                                             done_check="Authored isolated original-runtime traces")))
        self.assertTrue(batch.needs_user(row("HST-05", "HST", "godot-host",
                                             title="Verify audio reaches a physical speaker")))
        self.assertFalse(batch.needs_user(row("CMB-10", "CMB", "game")))

    def test_cheaper_rows_come_first_then_rows_that_unlock_more(self):
        rows = [
            row("A-01", "CMB", "game", difficulty="high"),
            row("A-02", "CMB", "game", difficulty="medium"),
            row("A-03", "DLG", "game", difficulty="medium"),
            row("A-04", "DLG", "game", status="blocked", depends_on="A-03"),
        ]
        self.assertEqual([r["id"] for r in batch.ready_rows(rows)], ["A-03", "A-02", "A-01"])


class PlanTests(unittest.TestCase):
    def test_a_batch_takes_one_row_per_area_and_one_host_row(self):
        rows = [
            row("C-01", "CMB", "game", difficulty="medium"),
            row("C-02", "CMB", "game", difficulty="medium"),
            row("U-01", "UI", "godot-host,game", difficulty="medium"),
            row("R-01", "RND", "godot-host", difficulty="medium"),
            row("F-01", "FMT", "esm", difficulty="high"),
            row("P-01", "PRG", "game", difficulty="high", done_check="original-runtime traces"),
        ]
        self.assertEqual([r["id"] for r in batch.plan_batch(rows, 4)], ["C-01", "U-01", "F-01"])

    def test_skipped_rows_are_left_out(self):
        rows = [row("C-01", "CMB", "game"), row("C-02", "CMB", "game")]
        self.assertEqual([r["id"] for r in batch.plan_batch(rows, 4, {"C-01"})], ["C-02"])


if __name__ == "__main__":
    unittest.main()

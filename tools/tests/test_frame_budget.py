"""PRF-16 contracts for thresholds, complete scenarios and final-sweep rows."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("frame_budget", ROOT / "tools/frame_budget.py")
perf = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(perf)


def sample(scenario="bannered-mare"):
    return dict(
        scenario=scenario, samples=3600, game_ticks=3600, elapsed_s=60.1, simulated_s=60,
        actors=1000 if scenario == "thousand-actors" else 8, evaluations=4000,
        navmesh=True, walked=scenario == "walk-out", cell="WhiterunWorld", exterior_actors=8,
        exterior_evaluations=4000 if scenario == "walk-out" else 0,
        world_editor_id="WhiterunWorld",
        frame=dict(p50_ms=1, p99_ms=2, worst_ms=3),
        systems={system: dict(p50_ms=0.1, p99_ms=0.2, worst_ms=0.3) for system in perf.SYSTEMS},
    )


class FrameBudgetTests(unittest.TestCase):
    def setUp(self):
        self.limits = perf.budgets(ROOT / "docs/performance.md")

    def test_prf_16_checks_every_system_and_all_three_budgets(self):
        for system in ("frame", *perf.SYSTEMS):
            for metric in perf.METRICS:
                with self.subTest(system=system, metric=metric):
                    row = sample()
                    entry = row["frame"] if system == "frame" else row["systems"][system]
                    entry.update(self.limits[system])
                    self.assertEqual(perf.assess(row, "bannered-mare", self.limits), [])
                    entry[metric] += 0.000001
                    if metric == "p50_ms":
                        entry["p99_ms"] = max(entry["p99_ms"], entry[metric])
                    entry["worst_ms"] = max(entry.values())
                    self.assertTrue(any(f"{system}.{metric}" in reason for reason in
                                        perf.assess(row, "bannered-mare", self.limits)))

    def test_prf_16_rejects_incomplete_invalid_idle_and_short_scenarios(self):
        mutations = (
            ("samples", 0), ("samples", True), ("game_ticks", 0), ("game_ticks", 7200),
            ("elapsed_s", 59.99), ("simulated_s", 59.99),
            ("actors", 0), ("evaluations", 0), ("navmesh", False), ("systems", {}),
            ("frame", dict(p50_ms=True, p99_ms=1, worst_ms=2)),
            ("frame", dict(p50_ms=3, p99_ms=1, worst_ms=2)),
            ("frame", dict(p50_ms=0, p99_ms=1, worst_ms=float("nan"))),
        )
        for name, value in mutations:
            with self.subTest(name=name, value=value):
                row = sample()
                row[name] = value
                with self.assertRaises(ValueError):
                    perf.assess(row, "bannered-mare", self.limits)
        for system in perf.SYSTEMS:
            row = sample()
            del row["systems"][system]
            with self.assertRaisesRegex(ValueError, "incomplete"):
                perf.assess(row, "bannered-mare", self.limits)
        for system in ("ai_planning", "ai_movement"):
            row = sample()
            row["systems"][system] = dict.fromkeys(perf.METRICS, 0)
            with self.assertRaisesRegex(ValueError, "inactive"):
                perf.assess(row, "bannered-mare", self.limits)
        row = sample("thousand-actors")
        row["actors"] = 999
        with self.assertRaisesRegex(ValueError, "thousand"):
            perf.assess(row, "thousand-actors", self.limits)
        row = sample("walk-out")
        row["walked"] = False
        with self.assertRaisesRegex(ValueError, "walk"):
            perf.assess(row, "walk-out", self.limits)

    def test_prf_16_walk_requires_resident_npc_and_exterior_package_evaluations(self):
        row = sample("walk-out")
        row["exterior_actors"] = 2
        self.assertEqual(perf.assess(row, "walk-out", self.limits), [])
        for name, value in (("exterior_actors", 1), ("exterior_evaluations", 0),
                            ("exterior_evaluations", None), ("exterior_evaluations", True)):
            with self.subTest(name=name, value=value):
                invalid = {**row, name: value}
                with self.assertRaisesRegex(ValueError, "exterior walk"):
                    perf.assess(invalid, "walk-out", self.limits)

    def test_prf_16_rejects_crashes_script_errors_and_missing_reports(self):
        output = "SOE_BUDGET " + json.dumps(sample())
        for text, code in ((output, 1), (output + "\nSCRIPT ERROR: bad binding", 0),
                           (output + "\nERROR: Godot binding failed", 0),
                           ("", 0), (output + "\n" + output, 0), ("SOE_BUDGET null", 0)):
            with self.assertRaises(ValueError):
                perf.parse_output(text, "bannered-mare", self.limits, code)

    def test_prf_16_keeps_completed_sample_with_papyrus_runtime_diagnostics(self):
        output = "ERROR: 6040 ERROR papyrus message=\"cannot call Disable() on a None object\"\n"
        output += "SOE_BUDGET " + json.dumps(sample())
        row, reasons = perf.parse_output(output, "bannered-mare", self.limits, 0)
        self.assertEqual(row["samples"], 3600)
        self.assertEqual(reasons, [])

    def test_prf_16_sweep_runs_all_scenarios_emits_rows_and_does_not_block(self):
        backlog_before = (ROOT / "BACKLOG.csv").read_bytes()
        for sweep in (False, True):
            with tempfile.TemporaryDirectory(dir=ROOT / "target") as directory:
                calls = []

                def execute(command, _env, log):
                    calls.append(command)
                    if "--import" in command:
                        output = "import complete"
                    else:
                        scenario = "thousand-actors" if "frame-budget" in command else command[-1]
                        row = sample(scenario)
                        if scenario == "bannered-mare":
                            row["systems"]["ai_planning"]["worst_ms"] = 9
                        output = "SOE_BUDGET " + json.dumps(row)
                    log.write_text(output)
                    return 0, output

                code = perf.run_suite(Path("godot"), Path("bench"), {"SKYRIM_DATA": "fixture"},
                                      Path(directory), sweep, execute)
                self.assertEqual(code, 0 if sweep else 2)
                self.assertEqual(len(calls), 4)
                report = json.loads((Path(directory) / "report.json").read_text())
                self.assertEqual([result["scenario"] for result in report["results"]], list(perf.SCENARIOS))
                self.assertEqual([result["status"] for result in report["results"]], ["breach", "pass", "pass"])
                self.assertEqual(len(report["followup_rows"]), 1)
                row = report["followup_rows"][0]
                self.assertRegex(row["id"], r"^PRF-\d{2}$")
                self.assertEqual(row["status"], "open")
                self.assertEqual(row["depends_on"], "PRF-16")
                self.assertTrue((Path(directory) / "follow-up-rows.csv").exists())
        self.assertEqual((ROOT / "BACKLOG.csv").read_bytes(), backlog_before)

    def test_prf_16_success_and_error_exit_statuses_are_distinct(self):
        for error in (False, True):
            with tempfile.TemporaryDirectory(dir=ROOT / "target") as directory:
                def execute(command, _env, log):
                    scenario = "thousand-actors" if "frame-budget" in command else command[-1]
                    output = "" if "--import" in command else "SOE_BUDGET " + json.dumps(sample(scenario))
                    code = 1 if error and scenario == "walk-out" else 0
                    log.write_text(output)
                    return code, output

                code = perf.run_suite(Path("godot"), Path("bench"), {}, Path(directory), False, execute)
                self.assertEqual(code, 1 if error else 0)

    def test_prf_16_orchestrator_final_sweep_invokes_fixed_suite(self):
        spec = importlib.util.spec_from_file_location("performance", ROOT / "tools/performance.py")
        orchestrator = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(orchestrator)
        with mock.patch.object(orchestrator.subprocess, "run") as execute:
            execute.return_value.returncode = 0
            code = orchestrator.main(["final-sweep", "--out", "target/final-budget"])
            self.assertEqual(code, 0)
            command = execute.call_args.args[0]
            self.assertEqual(command[0], str(ROOT / "tools/perf-budget.sh"))
            self.assertIn("--sweep", command)


if __name__ == "__main__":
    unittest.main()

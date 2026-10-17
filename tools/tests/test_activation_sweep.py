"""Activation report failures must remain visible after a child has exited."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("sweep", Path(__file__).parents[1] / "activation_sweep.py")
SWEEP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SWEEP)


class ActivationReportTests(unittest.TestCase):
    def test_truncated_child_report_returns_failure_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "host.json"
            path.write_text('{"complete":')
            data, error = SWEEP.read_host(path)
        self.assertEqual(data, {})
        self.assertIsNotNone(error)

    def test_error_deltas_retain_repeated_errors_without_prior_history(self):
        rows = [
            {"before": {"papyrus_errors": ["startup"]}, "after": {"papyrus_errors": ["startup", "same", "same"]}},
            {"before": {"papyrus_errors": ["startup", "same", "same"]}, "after": {"papyrus_errors": ["startup", "same", "same", "next"]}},
        ]
        report = SWEEP.compact_host({"references": rows})
        self.assertEqual(report["initial"]["papyrus_errors"], ["startup"])
        self.assertEqual([row["papyrus_errors"] for row in report["references"]], [["same", "same"], ["next"]])
        self.assertNotIn("papyrus_errors", report["references"][1]["before"])

    def test_already_compact_scene_errors_survive_loading(self):
        source = {"complete": True, "initial": {"papyrus_errors": ["startup"]}, "references": [
            {"reference": {"id": "0:000001"}, "before": {"papyrus_errors_count": 1}, "after": {"papyrus_errors_count": 2}, "papyrus_errors": ["new"]}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "host.json"
            path.write_text(json.dumps(source))
            report, error = SWEEP.read_host(path)
        self.assertIsNone(error)
        self.assertEqual(report["references"][0]["papyrus_errors"], ["new"])

    def test_valid_json_without_observations_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "host.json"
            path.write_text('{"complete": true, "references": "missing"}')
            data, error = SWEEP.read_host(path)
        self.assertEqual(data, {})
        self.assertIsNotNone(error)

    def test_binding_error_fails_own_model_without_losing_process_completion(self):
        rows = [{"reference": {"id": key}, "status": "pass", "cause": "state_changed", "model": model} for key, model in [("0:000001", "Furniture/Forge.nif"),("0:000002", "DweButton.nif")]]
        cell = {"cell": {"id": "0:000010"}, "references": rows}
        observations = {"cell": cell["cell"], "complete": True, "references": [{"reference": row["reference"], "before": {}, "after": {}, "causes": []} for row in rows]}
        error = 'ERROR host::object_behavior model="Furniture/Forge.nif" error="no matched node"'
        SWEEP.merge_host(cell, observations, 0, error + "\nACTIVATION_SWEEP complete", 1.0)
        self.assertTrue(cell["host"]["complete"])
        self.assertEqual(rows[0]["status"], "error")
        self.assertIn("host_behavior_binding_error", rows[0]["visual"]["causes"])
        self.assertEqual(rows[1]["status"], "pass")
        self.assertEqual(rows[1]["visual"]["causes"], [])

    def test_wrong_cell_cannot_complete_even_with_matching_references(self):
        row = {"reference": {"id": "0:000001"}, "status": "pass", "cause": "state_changed"}
        cell = {"cell": {"id": "0:000010"}, "references": [row]}
        observation = {"reference": row["reference"], "before": {}, "after": {}, "causes": []}
        SWEEP.merge_host(cell, {"cell": {"id": "0:000011"}, "complete": True, "references": [observation]}, 0, "ACTIVATION_SWEEP complete", 1.0)
        self.assertFalse(cell["host"]["complete"])
        self.assertEqual(row["status"], "error")

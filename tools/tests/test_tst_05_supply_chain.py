"""Keep both dependency audit tools in the commit gate."""

from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[2]


class SupplyChainGateTests(unittest.TestCase):
    def run_audit_step(self, tool, installed, exit_code=0):
        lint_script = (ROOT / "tools/lint.sh").read_text()
        label = "cargo deny check" if tool == "deny" else "cargo audit"
        audit_step = lint_script.split(f'step "{label}"\n', 1)[1].split('\nstep "', 1)[0]
        script = (
            "set -euo pipefail\n"
            f"command() {{ return {0 if installed else 1}; }}\n"
            f'cargo() {{ echo "$*"; return {exit_code}; }}\n'
            f"{audit_step}\n"
            "echo continued\n"
        )
        return subprocess.run(["bash", "-c", script], capture_output=True, text=True, check=False)

    def test_tst_05_gate_runs_installed_audit_tools(self):
        for tool, arguments in (("deny", "deny check"), ("audit", "audit")):
            with self.subTest(tool=tool):
                result = self.run_audit_step(tool, installed=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, f"{arguments}\ncontinued\n")

    def test_tst_05_gate_skips_missing_audit_tools(self):
        for tool in ("deny", "audit"):
            with self.subTest(tool=tool):
                result = self.run_audit_step(tool, installed=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, f"cargo-{tool} not installed, skipping\ncontinued\n")

    def test_tst_05_gate_stops_on_audit_failure(self):
        for tool in ("deny", "audit"):
            with self.subTest(tool=tool):
                result = self.run_audit_step(tool, installed=True, exit_code=7)
                self.assertEqual(result.returncode, 7, result.stderr)
                self.assertNotIn("continued", result.stdout)

    def test_tst_05_ci_installs_pinned_audit_tools(self):
        workflow = (ROOT / ".github/workflows/lint.yml").read_text()

        self.assertIn("cargo install --locked cargo-deny --version 0.20.2", workflow)
        self.assertIn("cargo install --locked cargo-audit --version 0.22.2", workflow)

    def test_tst_05_known_advisory_exception_has_a_reason(self):
        deny_config = (ROOT / "deny.toml").read_text()

        self.assertIn('id = "RUSTSEC-2024-0436"', deny_config)
        self.assertIn("no safe upgrade available", deny_config)


if __name__ == "__main__":
    unittest.main()

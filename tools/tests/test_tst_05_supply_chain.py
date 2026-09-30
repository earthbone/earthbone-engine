"""Keep both dependency audit tools in the commit gate."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


class SupplyChainGateTests(unittest.TestCase):
    def test_tst_05_gate_runs_cargo_deny(self):
        lint_script = (ROOT / "tools/lint.sh").read_text()

        self.assertIn('step "cargo deny check"\ncargo deny check', lint_script)

    def test_tst_05_gate_runs_cargo_audit(self):
        lint_script = (ROOT / "tools/lint.sh").read_text()

        self.assertIn('step "cargo audit"\ncargo audit', lint_script)

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

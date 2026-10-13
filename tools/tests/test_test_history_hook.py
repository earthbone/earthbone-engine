"""Prove optional history recording preserves gate output and exit status."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class HistoryHookTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="soe-history-hook-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        tools = self.root / "tools"
        tools.mkdir()
        (tools / "tests").mkdir()
        (tools / "tests/test_fixture.py").write_text(
            "import os\nimport unittest\nclass Fixture(unittest.TestCase):\n"
            "    def test_synthetic_check(self):\n"
            "        self.assertTrue(os.path.isfile('tools/lint.sh'))\n"
        )
        shutil.copy(Path(__file__).resolve().parents[1] / "lint.sh", tools / "lint.sh")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.script(
            self.bin / "cargo",
            '#!/bin/bash\n'
            'echo "cargo $*"\n'
            'echo "cargo stderr $*" >&2\n'
            'case "$*" in\n'
            '  *"--all-features"*) exit "${FAKE_FAILURE:-0}" ;;\n'
            'esac\n',
        )
        for name in ("cargo-deny", "cargo-audit", "comment-lint", "taplo", "markdownlint-cli2"):
            self.script(self.bin / name, "#!/bin/sh\nexit 0\n")
        self.script(tools / "prose-lint.sh", "#!/bin/sh\nexit 0\n")
        self.script(
            tools / "test-history-record.sh",
            '#!/bin/bash\n'
            'printf "%s %s\\n" "$1" "$3" >> "$RECORD_CALLS"\n'
            'echo optional-recorder-output\n'
            'exit 39\n',
        )
        self.calls = self.root / "calls.txt"

    @staticmethod
    def script(path, text):
        path.write_text(text)
        path.chmod(0o755)

    def gate(self, history, failure=0):
        environment = os.environ.copy()
        environment.pop("SOE_TEST_HISTORY", None)
        environment["PATH"] = str(self.bin) + os.pathsep + environment["PATH"]
        environment["RECORD_CALLS"] = str(self.calls)
        environment["FAKE_FAILURE"] = str(failure)
        if history:
            environment["SOE_TEST_HISTORY"] = str(self.root / "history.db")
        return subprocess.run(
            ["bash", "tools/lint.sh"], cwd=self.root, env=environment,
            capture_output=True, text=True, check=False,
        )

    def test_unset_history_is_silent_and_makes_no_calls(self):
        result = self.gate(False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.calls.exists())

    def test_optional_recorder_failure_preserves_output_and_success(self):
        before = self.gate(False)
        after = self.gate(True)
        self.assertEqual(after.returncode, before.returncode)
        self.assertEqual(after.stdout, before.stdout)
        self.assertEqual(after.stderr, before.stderr)
        self.assertEqual(
            self.calls.read_text().splitlines(),
            ["clippy-prod 0", "clippy-all 0", "clippy-nodef 0", "tier1 0"],
        )

    def test_failed_step_records_once_and_retains_exit(self):
        result = self.gate(True, failure=17)
        self.assertEqual(result.returncode, 17)
        self.assertEqual(self.calls.read_text().splitlines(), ["clippy-prod 0", "clippy-all 17"])


if __name__ == "__main__":
    unittest.main()

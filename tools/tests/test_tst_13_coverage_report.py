"""TST-13: workspace coverage report generated, gaps recorded in TEST_LOG.md.

Checks the recorded entry against TEST_PLAN.md's coverage row (section 1,
row "coverage") and section 1.2's three categories that line coverage cannot
show on its own (unsafe, concurrency, foreign code). This is a documentation
check, not a code test: it reads the report entry cargo-llvm-cov produced and
confirms the required content is there, it does not run cargo-llvm-cov itself.
"""

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
TEST_LOG = ROOT / "TEST_LOG.md"


def coverage_section(text):
    match = re.search(
        r"^## \d+\. Workspace, line coverage baseline\n(.*?)(?=\n## \d+\. |\Z)",
        text,
        re.DOTALL | re.MULTILINE,
    )
    return match.group(0) if match else None


class Tst13CoverageReportTests(unittest.TestCase):
    def setUp(self):
        self.text = TEST_LOG.read_text(encoding="utf-8")
        section = coverage_section(self.text)
        self.assertIsNotNone(
            section, "TEST_LOG.md has no 'Workspace, line coverage baseline' entry"
        )
        assert section is not None  # narrows for the type checker, message above already failed
        self.section = section

    def test_tst_13_coverage_section_exists_and_cites_the_tool(self):
        self.assertIn("cargo-llvm-cov", self.section)
        self.assertIn("cargo llvm-cov", self.section)

    def test_tst_13_workspace_totals_are_recorded(self):
        # Overall line, region and function coverage, as three distinct
        # percentages, not just one headline number.
        percentages = re.findall(r"\d{1,3}\.\d\d%", self.section)
        self.assertGreaterEqual(
            len(percentages),
            3,
            "expected at least a line, region and function coverage percentage",
        )

    def test_tst_13_per_crate_table_names_the_lowest_covered_crates(self):
        self.assertIn("godot-host", self.section)
        self.assertIn("bench", self.section)
        self.assertIn("|", self.section, "expected a per-crate markdown table")

    def test_tst_13_lowest_covered_parser_files_are_named_as_gaps(self):
        for gap_file in (
            "save/src/reference/extra.rs",
            "save/src/reference/fields.rs",
            "physics/src/shape.rs",
            "esm/src/tag.rs",
            "dds/src/format.rs",
            "papyrus/src/vm.rs",
        ):
            self.assertIn(gap_file, self.section, f"gap file {gap_file} not recorded")

    def test_tst_13_maps_to_the_three_section_1_2_categories(self):
        lowered = self.section.lower()
        self.assertIn("unsafe", lowered)
        self.assertIn("foreign code", lowered)
        # The concurrency category is addressed through the fan-out
        # primitive TEST_PLAN's risk actually names, not the bare word.
        self.assertTrue(
            "thread::scope" in self.section or "concurrency" in lowered,
            "section 1.2's concurrency category not addressed",
        )

    def test_tst_13_states_what_line_coverage_cannot_show(self):
        # The row's own scope note: a covered line is not a verified line.
        lowered = self.section.lower()
        self.assertIn("cannot", lowered)
        self.assertTrue(
            "asserted" in lowered or "correctness" in lowered,
            "expected the report to note coverage does not establish correctness",
        )


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


GENERATOR = Path(__file__).resolve().parents[1] / "update_task_index.py"


class UpdateTaskIndexTests(unittest.TestCase):
    def test_index_reports_pending_deferred_human_checks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            (root / "docs" / "task-Q3-dependabot.md").write_text(
                textwrap.dedent(
                    """\
                    ---
                    id: "Q3"
                    slug: dependabot
                    status: IMPLEMENTED
                    depends_on: ["Q2"]
                    ---

                    # Q3

                    ## Acceptance Criteria

                    ### Human — Deferred
                    Due: within 7 days

                    - [ ] AC-4: Dependabot PR生成を確認する。
                    - [x] AC-5: 別の遅延確認は完了した。
                    """
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                [sys.executable, str(GENERATOR)],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            index = (root / "docs" / "task-INDEX.md").read_text(encoding="utf-8")
            self.assertIn("| Q3 | dependabot | IMPLEMENTED |", index)
            self.assertIn("| 1 pending |", index)


if __name__ == "__main__":
    unittest.main()

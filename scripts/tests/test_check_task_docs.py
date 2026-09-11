from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


VALIDATOR = Path(__file__).resolve().parents[1] / "check_task_docs.py"
GENERATOR = Path(__file__).resolve().parents[1] / "update_task_index.py"


def task_doc(modify_path: str, deferred: str = "- なし。") -> str:
    return textwrap.dedent(
        f"""\
        ---
        id: "Q3"
        slug: fixture
        status: DRAFT
        depends_on: ["Q2"]
        ---

        # Task Q3

        ## Goal
        検証契約を確認する。

        ## Scope
        - fixtureだけを検証する。

        ## Out of Scope
        - application codeは変更しない。

        ## Invariants
        - production secretを使用しない。

        ## Files

        ### Modify
        - `{modify_path}`

        ### Add
        - なし。

        ## Required Tests
        - validatorがexit 0になる。

        ## Acceptance Criteria

        ### Codex
        - [ ] AC-1: validatorが契約を検査する。

        ### Human — Immediate
        - なし。

        ### Human — Deferred
        {deferred}

        ## Design Decisions
        - fixtureだけを検証する。

        ## Likely Pitfalls
        - pathの存在確認を省略しない。

        ## Reproduction / Verify
        - `make verify`

        ## Spec Deviations
        - なし。

        ## Implementation Record
        Changed:
        - 未実装。

        Decision:
        - 未実装。

        Impact:
        - API: none

        Verify:
        - `make verify`: NOT RUN

        Remaining:
        - implementation。

        ## Review Record
        Verdict: NOT REVIEWED

        ## Handoff
        - Branch: `test/fixture`
        - Commit message file: `docs/commits/task-Q3.txt`
        """
    )


class CheckTaskDocsTests(unittest.TestCase):
    def run_validator(
        self,
        content: str,
        *,
        stale_index: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            (root / "docs" / "commits").mkdir()
            (root / "docs" / "commits" / "task-Q3.txt").write_text(
                "test(ai): validate task contract\n", encoding="utf-8"
            )
            (root / "src").mkdir()
            (root / "src" / "existing.rs").write_text("", encoding="utf-8")
            (root / "docs" / "task-Q3-fixture.md").write_text(content, encoding="utf-8")
            subprocess.run(
                [sys.executable, str(GENERATOR)],
                cwd=root,
                capture_output=True,
                text=True,
                check=True,
            )
            if stale_index:
                (root / "docs" / "task-INDEX.md").write_text("stale\n", encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(VALIDATOR)],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )

    def test_existing_modify_path_passes(self) -> None:
        result = self.run_validator(task_doc("src/existing.rs"))

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_modify_path_fails(self) -> None:
        result = self.run_validator(task_doc("src/missing.rs"))

        self.assertEqual(result.returncode, 1)
        self.assertIn("Modify pathが存在しません", result.stderr)

    def test_deferred_human_check_requires_due(self) -> None:
        result = self.run_validator(
            task_doc("src/existing.rs", "- [ ] AC-2: 後日動作を確認する。")
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("Human — Deferred ACにはDueが必要です", result.stderr)

    def test_lowercase_status_fails(self) -> None:
        content = task_doc("src/existing.rs").replace("status: DRAFT", "status: draft")
        result = self.run_validator(content)

        self.assertEqual(result.returncode, 1)
        self.assertIn("不正なstatus", result.stderr)

    def test_implemented_requires_checked_codex_ac(self) -> None:
        content = task_doc("src/existing.rs").replace(
            "status: DRAFT", "status: IMPLEMENTED"
        )
        result = self.run_validator(content)

        self.assertEqual(result.returncode, 1)
        self.assertIn("未完了Codex AC", result.stderr)

    def test_blocked_requires_concrete_reason(self) -> None:
        content = task_doc("src/existing.rs").replace(
            "status: DRAFT", "status: BLOCKED"
        )
        result = self.run_validator(content)

        self.assertEqual(result.returncode, 1)
        self.assertIn("具体的な停止理由", result.stderr)

    def test_done_allows_unchecked_deferred_human_check(self) -> None:
        content = task_doc(
            "src/existing.rs",
            "Due: within 7 days\n        \n        - [ ] AC-2: 後日動作を確認する。",
        )
        content = content.replace("status: DRAFT", "status: DONE")
        content = content.replace("- [ ] AC-1:", "- [x] AC-1:")
        content = content.replace("`make verify`: NOT RUN", "`make verify`: PASS")
        content = content.replace("Verdict: NOT REVIEWED", "Verdict: READY")
        result = self.run_validator(content)

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_stale_task_index_fails(self) -> None:
        result = self.run_validator(task_doc("src/existing.rs"), stale_index=True)

        self.assertEqual(result.returncode, 1)
        self.assertIn("task statusまたはDeferred ACと不整合", result.stderr)


if __name__ == "__main__":
    unittest.main()

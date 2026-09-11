from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import unittest


RUNNER = Path(__file__).resolve().parents[1] / "run_quiet.py"


class RunQuietTests(unittest.TestCase):
    def run_runner(self, program: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(RUNNER),
                "fixture",
                "--tail-lines",
                "2",
                "--",
                sys.executable,
                "-c",
                program,
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_success_keeps_only_the_last_non_empty_line(self) -> None:
        result = self.run_runner("print('noisy line'); print('summary line')")

        self.assertEqual(result.returncode, 0)
        self.assertIn("fixture: PASS (exit 0)", result.stdout)
        self.assertEqual(
            result.stdout.splitlines()[-2:],
            ["fixture: PASS (exit 0)", "summary line"],
        )

    def test_failure_preserves_exit_code_and_only_prints_tail(self) -> None:
        result = self.run_runner(
            "import sys; print('old'); print('cause'); print('detail'); sys.exit(7)"
        )

        self.assertEqual(result.returncode, 7)
        self.assertIn("fixture: FAIL (exit 7)", result.stderr)
        self.assertIn("cause", result.stderr)
        self.assertIn("detail", result.stderr)
        self.assertNotIn("old", result.stderr)


if __name__ == "__main__":
    unittest.main()

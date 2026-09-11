#!/usr/bin/env python3
"""Run a command while keeping successful AI/CI logs compact.

Success prints the command, exit code, PASS, and only the last non-empty output
line. Failure prints the tail needed for diagnosis and preserves the exit code.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shlex
import subprocess
import sys


def parse_args() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError:
        separator = len(sys.argv)

    parser = argparse.ArgumentParser()
    parser.add_argument("label")
    parser.add_argument("--cwd", type=Path)
    parser.add_argument("--env", action="append", default=[], metavar="KEY=VALUE")
    parser.add_argument("--unset-env", action="append", default=[], metavar="KEY")
    parser.add_argument("--tail-lines", type=int, default=120)
    args = parser.parse_args(sys.argv[1:separator])
    args.command = sys.argv[separator + 1 :] if separator < len(sys.argv) else []
    if not args.command:
        parser.error("command is required after --")
    if args.tail_lines < 1:
        parser.error("--tail-lines must be positive")

    return args


def command_display(command: list[str], cwd: Path | None) -> str:
    rendered = shlex.join(command)
    return f"(cd {shlex.quote(str(cwd))} && {rendered})" if cwd else rendered


def last_non_empty_line(output: str) -> str | None:
    return next((line for line in reversed(output.splitlines()) if line.strip()), None)


def main() -> int:
    args = parse_args()
    environment = os.environ.copy()

    for key in args.unset_env:
        environment.pop(key, None)
    for assignment in args.env:
        if "=" not in assignment:
            print(f"run_quiet: invalid --env value: {assignment!r}", file=sys.stderr)
            return 2
        key, value = assignment.split("=", 1)
        environment[key] = value

    print(f"$ {command_display(args.command, args.cwd)}")
    completed = subprocess.run(
        args.command,
        cwd=args.cwd,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
        check=False,
    )

    output = completed.stdout or ""
    if completed.returncode == 0:
        print(f"{args.label}: PASS (exit 0)")
        final_line = last_non_empty_line(output)
        if final_line:
            print(final_line)
        return 0

    print(f"{args.label}: FAIL (exit {completed.returncode})", file=sys.stderr)
    lines = output.splitlines()
    if lines:
        print(
            f"--- failure output: last {min(len(lines), args.tail_lines)} lines ---",
            file=sys.stderr,
        )
        print("\n".join(lines[-args.tail_lines :]), file=sys.stderr)
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
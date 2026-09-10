#!/usr/bin/env bash
set -euo pipefail

project_root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"

# ops-hub は Rust backend が back_cargo/ 配下にある。
if [[ -f "$project_root/Cargo.toml" ]]; then
  cargo_root="$project_root"
elif [[ -f "$project_root/back_cargo/Cargo.toml" ]]; then
  cargo_root="$project_root/back_cargo"
else
  exit 0
fi

# PostToolUse JSON から編集対象を取得し、
# Rust source の編集時だけ cargo fmt を実行する。
payload="$(cat || true)"

file_path="$(
  printf '%s' "$payload" |
    python3 -c '
import json
import sys

try:
    data = json.load(sys.stdin)
except (json.JSONDecodeError, EOFError):
    data = {}

print(data.get("tool_input", {}).get("file_path", ""))
' 2>/dev/null || true
)"

case "$file_path" in
  *.rs)
    cd "$cargo_root"
    cargo fmt --all
    ;;
  *)
    exit 0
    ;;
esac
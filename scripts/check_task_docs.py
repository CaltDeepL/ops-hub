#!/usr/bin/env python3
"""Task 07以降の機械可読task docを検証する。

重要:
- implementing/fixing中の未チェックACは正常。make verifyを邪魔しない。
- 厳格な完了条件は status=done の場合だけ要求する。
- Task 01〜06のlegacy文書はfrontmatterが無くても対象外。
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

ALLOWED_STATUSES = {
    "planned",
    "spec",
    "implementing",
    "blocked",
    "review",
    "fixing",
    "done",
}

failed = False


def fail(path: Path, message: str) -> None:
    global failed
    print(f"{path}: {message}", file=sys.stderr)
    failed = True


def section(text: str, heading: str) -> str | None:
    match = re.search(
        rf"^##\s+\d*\.?\s*{re.escape(heading)}\s*$\n(.*?)(?=^##\s|\Z)",
        text,
        re.MULTILINE | re.DOTALL,
    )
    if match:
        return match.group(1)

    # Numbering無しのheadingにも対応。
    match = re.search(
        rf"^##\s+{re.escape(heading)}\s*$\n(.*?)(?=^##\s|\Z)",
        text,
        re.MULTILINE | re.DOTALL,
    )
    return match.group(1) if match else None


for path in sorted(Path("docs").glob("task-[0-9][0-9]-*.md")):
    # Legacy Task 01-06 は既存成果物を凍結し、v3検証対象外。
    task_match = re.match(r"task-(\d{2})-", path.name)
    if not task_match:
        continue
    task_id = int(task_match.group(1))
    if task_id <= 6:
        continue

    text = path.read_text(encoding="utf-8")
    fm = re.match(r"^---\n(.*?)\n---\n", text, re.DOTALL)
    if not fm:
        fail(path, "Task 07以降はYAML frontmatterが必要です")
        continue

    meta = fm.group(1)

    def meta_value(key: str) -> str | None:
        m = re.search(rf"^{re.escape(key)}:\s*(.*?)\s*$", meta, re.MULTILINE)
        return m.group(1).strip() if m else None

    for key in ("id", "slug", "status", "depends_on"):
        if meta_value(key) is None:
            fail(path, f"frontmatterに {key} がありません")

    status = (meta_value("status") or "").strip("'\"")
    if status not in ALLOWED_STATUSES:
        fail(path, f"不正なstatus: {status!r}")
        continue

    ac = section(text, "Acceptance Criteria")
    if ac is None:
        fail(path, "Acceptance Criteria sectionがありません")
        continue

    criteria = re.findall(r"^- \[([ xX])\]\s+(AC-\d+):", ac, re.MULTILINE)
    if not criteria:
        fail(path, "AC-* 形式のAcceptance Criteriaがありません")

    if status != "done":
        # 開発途中はAC未完了を許可する。ここがv2からの重要修正点。
        continue

    unchecked = [ac_id for mark, ac_id in criteria if mark == " "]
    if unchecked:
        fail(path, f"status=done なのに未完了ACがあります: {', '.join(unchecked)}")

    deviations = section(text, "Spec Deviations")
    if deviations is None:
        fail(path, "Spec Deviations sectionがありません")
    else:
        bullets = re.findall(r"^-\s+(.+?)\s*$", deviations, re.MULTILINE)
        unresolved = [
            item for item in bullets if item.strip() not in {"なし。", "なし"}
        ]
        if unresolved:
            fail(path, "status=done なのにSpec Deviationsが未解決です")

    review = section(text, "Review Record")
    if review is None:
        fail(path, "Review Record sectionがありません")
        continue

    required_checks = (
        "`make verify` の実際の成功結果を確認した。",
        "全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。",
    )
    for label in required_checks:
        if not re.search(rf"^- \[[xX]\]\s+{re.escape(label)}\s*$", review, re.MULTILINE):
            fail(path, f"status=done にはReview verification必須: {label}")

    if not re.search(r"^- \[[xX]\]\s+READY\s*$", review, re.MULTILINE):
        fail(path, "status=done にはReview disposition READYが必要です")

    evidence = section(text, "Implementation Record")
    if evidence is None:
        fail(path, "Implementation Record sectionがありません")
    else:
        if "make verify" not in evidence:
            fail(path, "Implementation Recordにmake verify証拠がありません")
        if "<実際の終了結果" in evidence:
            fail(path, "Implementation Recordのverification placeholderが残っています")

sys.exit(1 if failed else 0)

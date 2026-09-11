#!/usr/bin/env python3
"""Managed task doc の機械可読性とv3状態契約を検証する。

Managed task:
- Q1, Q2, Q3, ...
- 07, 08, ... 16

Legacy task:
- 01〜06 は既存成果物を凍結し、検証対象外。
- Q1〜Q2 はv3 template導入前の履歴として構造だけ互換扱い。

重要:
- DRAFT/APPROVED/BLOCKEDの未チェックACは正常。
- IMPLEMENTED以降はCodex AC、READY以降はreview証拠を要求する。
- filename ID と frontmatter id は一致必須。
- Q3 / Task 07以降はv3 task structureと分類済みACが必須。
- Files/Modifyは実在path、Files/Addは実在parentを要求する。
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

from update_task_index import render_task_index


ALLOWED_STATUSES = {
    "DRAFT",
    "APPROVED",
    "IMPLEMENTED",
    "READY",
    "DONE",
    "BLOCKED",
}

TASK_RE = re.compile(
    r"^task-(?P<id>Q[1-9]\d*|\d{2})-.*\.md$",
    re.IGNORECASE,
)

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


def subsection(text: str, heading: str) -> str | None:
    match = re.search(
        rf"^###\s+{re.escape(heading)}\s*$\n(.*?)(?=^###\s|\Z)",
        text,
        re.MULTILINE | re.DOTALL,
    )
    return match.group(1) if match else None


def is_managed_task(task_id: str) -> bool:
    """v3 validator の管理対象 task ID か判定する。"""
    normalized = task_id.upper()

    if normalized.startswith("Q"):
        # TASK_RE が Q[1-9]\d* を保証している。
        return True

    # 数値系列は Task 07〜16 のみ managed。
    numeric_id = int(normalized)
    return 7 <= numeric_id <= 16


def uses_v3_structure(task_id: str) -> bool:
    """新構造を適用する。Q1/Q2は移行前の履歴なので書き換えない。"""
    normalized = task_id.upper()
    if normalized.startswith("Q"):
        return int(normalized[1:]) >= 3
    return int(normalized) >= 7


def nonempty_bullets(text: str) -> list[str]:
    return [item.strip() for item in re.findall(r"^-\s+(.+?)\s*$", text, re.MULTILINE)]


def validate_task_contract(path: Path, text: str) -> None:
    headings = (
        "Goal",
        "Scope",
        "Out of Scope",
        "Invariants",
        "Files",
        "Required Tests",
        "Acceptance Criteria",
        "Design Decisions",
        "Likely Pitfalls",
        "Reproduction / Verify",
        "Spec Deviations",
        "Implementation Record",
        "Review Record",
        "Handoff",
    )
    parts: dict[str, str] = {}
    for heading in headings:
        value = section(text, heading)
        if value is None or not value.strip():
            fail(path, f"{heading} sectionがありません")
        else:
            parts[heading] = value

    for heading in ("Goal", "Scope", "Out of Scope"):
        value = parts.get(heading, "")
        if "<" in value or ">" in value:
            fail(path, f"{heading}にplaceholderが残っています")

    for heading in ("Scope", "Out of Scope", "Invariants", "Required Tests"):
        value = parts.get(heading, "")
        bullets = nonempty_bullets(value)
        if not bullets or all(item in {"なし", "なし。"} for item in bullets):
            fail(path, f"{heading}に具体的な項目がありません")

    files = parts.get("Files", "")
    modify = subsection(files, "Modify")
    add = subsection(files, "Add")
    if modify is None or add is None:
        fail(path, "FilesにはModifyとAdd subsectionが必要です")
        return

    modify_paths = re.findall(r"^-\s+`([^`]+)`(?:\s+—.*)?$", modify, re.MULTILINE)
    add_paths = re.findall(r"^-\s+`([^`]+)`(?:\s+—.*)?$", add, re.MULTILINE)
    if not modify_paths and not add_paths:
        fail(path, "Filesには具体的なModifyまたはAdd pathが必要です")

    for raw_path in modify_paths + add_paths:
        if any(character in raw_path for character in "*?[]<>"):
            fail(path, f"Files pathは具体的にしてください: {raw_path!r}")
            continue
        target = Path(raw_path)
        if raw_path in modify_paths and not target.exists():
            fail(path, f"Modify pathが存在しません: {raw_path}")
        if raw_path in add_paths and not target.parent.exists():
            fail(path, f"Add pathの親directoryが存在しません: {raw_path}")

    if "make verify" not in parts.get("Reproduction / Verify", ""):
        fail(path, "Reproduction / Verifyにmake verifyがありません")


for path in sorted(Path("docs").glob("task-*.md")):
    task_match = TASK_RE.match(path.name)
    if not task_match:
        continue

    filename_id = task_match.group("id")

    if not is_managed_task(filename_id):
        # Legacy Task 01〜06、および現時点で管理対象外の数値taskは無視する。
        continue

    text = path.read_text(encoding="utf-8")
    validation_text = re.sub(r"<!--.*?-->", "", text, flags=re.DOTALL)

    fm = re.match(r"^---\n(.*?)\n---\n", text, re.DOTALL)
    if not fm:
        fail(path, f"Managed Task {filename_id} はYAML frontmatterが必要です")
        continue

    meta = fm.group(1)

    def meta_value(key: str) -> str | None:
        match = re.search(
            rf"^{re.escape(key)}:\s*(.*?)\s*$",
            meta,
            re.MULTILINE,
        )
        return match.group(1).strip() if match else None

    for key in ("id", "slug", "status", "depends_on"):
        if meta_value(key) is None:
            fail(path, f"frontmatterに {key} がありません")

    frontmatter_id = (meta_value("id") or "").strip("'\"")

    if frontmatter_id.upper() != filename_id.upper():
        fail(
            path,
            "filename ID と frontmatter id が一致しません: "
            f"filename={filename_id!r}, frontmatter={frontmatter_id!r}",
        )

    status = (meta_value("status") or "").strip("'\"")
    if status not in ALLOWED_STATUSES:
        fail(path, f"不正なstatus: {status!r}")
        continue

    uses_new_contract = uses_v3_structure(filename_id)
    if uses_new_contract:
        validate_task_contract(path, validation_text)

    commit_message = Path("docs/commits") / f"task-{filename_id.upper()}.txt"
    if not commit_message.exists() or not commit_message.read_text(encoding="utf-8").strip():
        fail(path, f"commit message fileがありません: {commit_message}")

    ac = section(validation_text, "Acceptance Criteria")
    if ac is None:
        fail(path, "Acceptance Criteria sectionがありません")
        continue

    criteria = re.findall(
        r"^- \[([ xX])\]\s+(AC-\d+):",
        ac,
        re.MULTILINE,
    )
    if not criteria:
        fail(path, "AC-* 形式のAcceptance Criteriaがありません")

    if uses_new_contract:
        codex_ac = subsection(ac, "Codex")
        immediate_ac = subsection(ac, "Human — Immediate")
        deferred_ac = subsection(ac, "Human — Deferred")

        for heading, value in (
            ("Codex", codex_ac),
            ("Human — Immediate", immediate_ac),
            ("Human — Deferred", deferred_ac),
        ):
            if value is None:
                fail(path, f"Acceptance Criteriaに {heading} subsectionがありません")

        if codex_ac is not None and not re.search(
            r"^- \[[ xX]\]\s+AC-\d+:", codex_ac, re.MULTILINE
        ):
            fail(path, "Acceptance CriteriaのCodex subsectionにAC-*がありません")

        if deferred_ac is not None:
            deferred_items = re.findall(
                r"^- \[([ xX])\]\s+(AC-\d+):",
                deferred_ac,
                re.MULTILINE,
            )
            if deferred_items and not re.search(
                r"^Due:\s*\S.+$", deferred_ac, re.MULTILINE
            ):
                fail(path, "Human — Deferred ACにはDueが必要です")

    if status == "BLOCKED":
        deviations = section(validation_text, "Spec Deviations")
        if deviations is None:
            fail(path, "BLOCKEDにはSpec Deviations sectionが必要です")
        else:
            reasons = [
                item for item in nonempty_bullets(deviations)
                if item not in {"なし", "なし。"}
            ]
            if not reasons:
                fail(path, "BLOCKEDには具体的な停止理由が必要です")
        continue

    if status in {"DRAFT", "APPROVED"}:
        continue

    codex_scope = ac
    if uses_new_contract:
        codex_scope = codex_ac or ""
    completion_criteria = re.findall(
        r"^- \[([ xX])\]\s+(AC-\d+):",
        codex_scope,
        re.MULTILINE,
    )
    unchecked = [ac_id for mark, ac_id in completion_criteria if mark == " "]
    if unchecked:
        fail(
            path,
            f"status={status} なのに未完了Codex ACがあります: "
            f"{', '.join(unchecked)}",
        )

    deviations = section(validation_text, "Spec Deviations")
    if deviations is None:
        fail(path, "Spec Deviations sectionがありません")
    else:
        bullets = re.findall(
            r"^-\s+(.+?)\s*$",
            deviations,
            re.MULTILINE,
        )
        unresolved = [
            item
            for item in bullets
            if item.strip() not in {"なし。", "なし"}
        ]
        if unresolved:
            fail(
                path,
                f"status={status} なのにSpec Deviationsが未解決です",
            )

    evidence = section(validation_text, "Implementation Record")
    if evidence is None:
        fail(path, "Implementation Record sectionがありません")
    else:
        if "make verify" not in evidence:
            fail(
                path,
                "Implementation Recordにmake verify証拠がありません",
            )
        if "<実際の終了結果" in evidence:
            fail(
                path,
                "Implementation Recordのverification placeholderが残っています",
            )
        if uses_new_contract:
            for label in ("Changed:", "Decision:", "Impact:", "Verify:", "Remaining:"):
                if label not in evidence:
                    fail(path, f"Implementation Recordに {label} がありません")
        if uses_new_contract:
            if not re.search(r"`make verify`:\s*PASS", evidence):
                fail(path, f"status={status}にはmake verify PASS証拠が必要です")
        elif "make verify" not in evidence:
            fail(path, f"status={status}にはmake verify証拠が必要です")

    if status not in {"READY", "DONE"}:
        continue

    review = section(validation_text, "Review Record")
    if review is None:
        fail(path, "READY/DONEにはReview Record sectionが必要です")
    elif not (
        re.search(r"^Verdict:\s*READY\s*$", review, re.MULTILINE)
        or re.search(r"^- \[[xX]\]\s+READY\s*$", review, re.MULTILINE)
    ):
        fail(path, "READY/DONEにはreview verdict READYが必要です")

    immediate_scope = immediate_ac if uses_new_contract else ac
    immediate_criteria = re.findall(
        r"^- \[([ xX])\]\s+(AC-\d+):",
        immediate_scope or "",
        re.MULTILINE,
    )
    unchecked_immediate = [
        ac_id for mark, ac_id in immediate_criteria if mark == " "
    ]
    if unchecked_immediate:
        fail(path, f"{status}なのに未完了Human Immediate ACがあります: " + ", ".join(unchecked_immediate))


index_path = Path("docs/task-INDEX.md")
if not index_path.exists():
    fail(index_path, "task indexがありません。make task-indexを実行してください")
elif index_path.read_text(encoding="utf-8") != render_task_index():
    fail(index_path, "task statusまたはDeferred ACと不整合です。make task-indexを実行してください")


sys.exit(1 if failed else 0)

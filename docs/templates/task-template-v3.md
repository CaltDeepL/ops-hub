---
id: "ID"
slug: short-slug
status: DRAFT
depends_on: []
---

# Task ID：<タイトル>

## Goal

<このtaskで達成する観測可能な結果。>

## Scope

- <今回変更する機能・範囲。>

## Out of Scope

- <今回変更しない機能・設定。>

## Invariants

- <壊してはいけない既存仕様。>
- productionの`DATABASE_URL`を使用しない。

## Files

### Modify

- `<実在するrepository/path>` — <変更目的>

### Add

- なし。

## Required Tests

- <実装すべき成功ケース。>
- <実装すべきfailure / concurrency / boundaryケース。>

## Acceptance Criteria

### Codex

- [ ] AC-1: <コード・設定・testで実証可能な条件。>
- [ ] AC-2: Required Testsが存在し、成功する。
- [ ] AC-3: `make verify`が成功する。

### Human — Immediate

- なし。

### Human — Deferred

- なし。

<!-- 遅延確認がある場合:
Due: within 7 days

- [ ] AC-4: <時間経過後にHumanが確認する条件。>
-->

## Design Decisions

- DD-1: <実装に必要な決定。>

## Likely Pitfalls

- <誤実装しやすい点と回避条件。>

## Reproduction / Verify

- `<狭いtest command>`
- `make verify`
- `make audit`（必要な場合だけ。`make verify`の代用ではない）

## Spec Deviations

- なし。

Invariants/AC/Scopeと衝突する、またはL2でも仕様が一意にならない場合は「何を・なぜ」を記録し、`BLOCKED`として停止する。

## Implementation Record

Changed:
- 未実装。

Decision:
- 未実装。

Impact:
- API: 未確認
- DB: 未確認
- Migration: 未確認
- SQLx: 未確認

Verify:
- `make verify`: NOT RUN
- `make audit`: not required

Remaining:
- Codex implementation。

## Review Record

Verdict: NOT REVIEWED

Evidence:
- なし。

Findings:
- なし。

## Handoff

- Branch: `<type/short-slug>`
- Commit message file: `docs/commits/task-ID.txt`
- Next: Human Gateで承認後、`/impl ID`。

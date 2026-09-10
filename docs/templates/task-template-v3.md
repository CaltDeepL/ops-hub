---
id: "NN"
slug: short-slug
status: spec
# planned -> spec -> implementing -> review -> fixing -> review -> done
# 設計矛盾時: blocked -> spec
depends_on: []
---

# タスクNN：<タイトル>

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | <要件/基本設計/詳細設計の章> |
| ゴール | <このtaskで1つだけ達成する結果> |
| 完了条件 | <最重要の観測可能な完了条件> |
| 実装範囲外 | <明示的non-goals> |

## 1. Context / 現状

<直前タスクから何が存在し、何がまだ無いか。>

## 2. Acceptance Criteria

- [ ] AC-1: <観測・テスト可能>
- [ ] AC-2: <観測・テスト可能>
- [ ] AC-3: <failure path>

## 3. 設計判断・不変条件

- DD-1: <Codexが変更してはいけない設計判断>
- DD-2: <不変条件>

必要なら処理順を書く。

```text
request
  ↓
...
```

## 4. 想定変更箇所

- `<path>` — <役割>

ファイル名の微調整は許容するが、DD-* を変える必要が出た場合は Spec Deviations を使う。

## 5. DB / migration / SQLx

<変更なし、またはmigration/constraint/backfill/.sqlxの扱い。>

## 6. API / 互換性

<status/body/auth/backward compatibility。>

## 7. ADR

- なし。

または

- `docs/adr/NNNN-*.md`

## 8. Spec Deviations

実装中に DD-* と矛盾する判断が必要になった場合、コードを書く前に「何を・なぜ」をここへ記録し、`status: blocked` にして停止する。Claude architectが設計を更新するまで再開しない。

- なし。

## 9. 検証

反復用の狭い検証:

```bash
<必要なtestだけ>
```

最終品質ゲート:

```bash
make verify
```

## 10. Implementation Record

_Codexが実装完了時に更新する。_

### 変更ファイル

- 

### 実装上の判断

- 

### DB / APIへの影響

- 

### Verification evidence

```text
make verify
<実際の終了結果と、必要な範囲の出力>
```

### 残課題

- なし。

## 11. Review Record

_Claude Code親セッションがread-only reviewerの結果を転記する。_

### Verification

- [ ] `make verify` の実際の成功結果を確認した。
- [ ] 全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。

### Acceptance evidence

| Criterion | Evidence (`file:line` / test / command) |
|---|---|
| AC-1 | |
| AC-2 | |
| AC-3 | |

### Findings

| ID | Severity | File:line | 失敗シナリオ / 指摘 | 必要な修正 | 根拠 |
|---|---|---|---|---|---|
| — | — | — | — | — | — |

### Review disposition

- [ ] BLOCKED
- [ ] CHANGES REQUESTED
- [ ] READY

## 12. つまずいた点と教訓

<実装時に発生した問題と、次回再発防止になる知識。>

## 13. 次タスクへの引き継ぎ

<次のtaskが知るべき確定事項。>

## 14. 再現コマンド

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
make verify
```

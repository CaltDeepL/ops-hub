# Task 18: back_cargo の復元

| 項目 | 内容 |
|---|---|
| 目的 | マージ事故で消えた `back_cargo/` を、削除前のコミットからそのまま復元する |
| 完了条件 | `back_cargo/` 37ファイルがコンフリクト無しで main に戻り、`cargo build` / `cargo test` が通る |
| 採番 | 旧採番 #8 から繰り上げ |
| ステータス | 完了（以降の Task 20 で CI 上の `make verify` が PASS） |

## 1. 実施内容

### 経緯：消失はマージ事故だった

直前の監査は「Rust バックエンド・DB・API は未配置」としていたが、実際には**マージコミットで丸ごと削除されていた**。`git log --all --oneline -- back_cargo` で追うと、`4e6802f`（`修正`、`infrastructure/ci-setup` ブランチとのマージ）を境に back_cargo 配下37ファイル・4806行が消えている。

| コミット | back_cargo ファイル数 |
|---|---:|
| `a772d7a` (task6まで巻き戻した) | 37 |
| `3269cb7` (yml修正) | 37 |
| `4e6802f` (修正・マージ) | **0** |
| 以降 `main` 先頭まで | 0 |

`4e6802f` は `3269cb7`（back_cargo あり）と `cf4a90c`（`infrastructure/ci-setup` 側、品質基盤スクリプト群を含む）のマージで、コンフリクト解決の過程で back_cargo 側が丸ごと落ちたと見られる。品質基盤スクリプト（`check_ruleset_contract.py` / `check_task_docs.py` / `scripts/github/main-ruleset.json` / `scripts/github/verify_main_ruleset.py` 等）はこのマージで正しく取り込まれており、削除は意図的な設計判断ではなく**マージ解決の事故**と判断した。

### 復元

削除前の最終コミット `3269cb7` から back_cargo をそのまま復元した。中身の変更は無し。

### 検証したこと

- `3269cb7` と `a772d7a` の back_cargo 配下は `git diff` で差分ゼロ（同一内容）
- 現在の `main` 先頭（`686e022`）に `3269cb7:back_cargo` を checkout → 37ファイル、コンフリクト無しでステージ完了
- `.gitignore`（`4e6802f` 時点で10行追加）に back_cargo 関連の除外記述が無い

## 2. 設計判断

- **新しく実装し直さず、履歴から復元する。** 消失原因が事故である以上、削除前の状態が正。チャット履歴から手動でファイルを再構築するより先に履歴を確認したことで、復元コストをゼロ（コピーのみ）にできた
- **中身は一切変更しない。** 復元と修正を混ぜると、差分レビューで「戻しただけ」を証明できなくなる

## 3. つまずいた点と教訓

- 監査（README）は「Rust バックエンドは未配置」という**現状の事実**を正しく報告していたが、その原因を「未着手」と誤解しないよう、**着手前に `git log --all -- <path>` で履歴を確認する**ことが重要だった
- `git log --all --oneline -- back_cargo` の出力を `&&` チェーンの `grep -c` に直結すると、0件ヒット時に exit code 1 で以降のコマンドが止まる（`grep -c` は0でも非ゼロ終了する）。ループで1コミットずつ確認する形に変えて対処した

## 4. 再現コマンド

```bash
cd ~/A/ops-hub

# 消失箇所の特定
git log --all --oneline -- back_cargo

# 復元（patch を使わない場合）
git checkout 3269cb7 -- back_cargo
git add back_cargo
git commit -F <commit message>

# 同梱 patch を使う場合
git am task-18-restore-back-cargo.patch
git commit --amend --reset-author   # コミット者情報がダミーのため

# 適用後の確認
cd back_cargo
cargo build
cargo test
cd ..
cat .github/workflows/security-audit.yml   # working-directory: back_cargo が解決していること
```

## 5. 次タスクへの引き継ぎ

- back_cargo 復元後、`security-audit.yml` の `working-directory: back_cargo` 参照は解決するはずだが、実際に CI を走らせて確認していない
- ci.yml はまだ setup のみで実質的な検証を行っていない。Task 19 で AI 協働フレームワークの残骸（`check_task_docs.py` / `update_task_index.py`）を削除し、Task 20 で ci.yml に make verify を組み込む
- Dependabot（`.github/dependabot.yml`）、Ruleset 差分（`strict required status checks` / `review thread resolution` / `unattributed changes への追加 approval`）は未着手
- `4e6802f` のようなマージ事故を再発させないため、`main-protection` Ruleset の `strict required status checks` を有効化する（→ Task 22 で live 側が `true` であることを確認し、正本もそれに合わせた）

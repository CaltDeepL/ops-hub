# Task 17: vite.config.ts の復元

| 項目 | 内容 |
|---|---|
| 目的 | 欠落していた `vite.config.ts` を復元し、`npm run build` を green に戻す |
| 完了条件 | `npm run dev` / `npm run build` / `npm run lint` がすべて成功する |
| 採番 | 品質基盤タスクの1番目。旧07は正式ロードマップの task-07（stale-lock-recovery）と衝突するため17に繰り上げ |
| ステータス | 完了 |

## 1. 実施内容

古いチェックアウト（GitHub `main` 先頭 `511fce6` 相当）に戻したところ、リポジトリ直下に `vite.config.ts` が存在しない状態だった。`tsconfig.node.json` の `include` が `["vite.config.ts"]` のみのため、このファイルが無いと

```
TS18003: No inputs were found in tsconfig.node.json (include: vite.config.ts)
```

で `npm run build`（`tsc -b && vite build`）が失敗する。`npm run dev` は Vite が tsconfig.node.json を経由せず直接起動するため影響を受けず、`npm run lint` も `vite.config.ts` を lint 対象に含めていないため影響を受けない。この非対称性が「lint 成功・build 失敗」の原因。

`plugins: [react()]` の最小構成で `vite.config.ts` を復元した。

## 2. 設計判断

- `package.json` の devDependencies に `@vitejs/plugin-react`（`-swc` 版ではない）のみが入っており、他の Vite plugin は無い。プラグイン構成は `plugins: [react()]` の最小形とした
- `tsconfig.app.json` / `tsconfig.node.json` のどちらにも `paths` エイリアス設定が無いため、`resolve.alias`（`@/...` 等）は追加していない。将来エイリアスを導入する場合は `tsconfig.app.json` 側の `paths` とセットで追加する
- `server` / `build` 等のオプションは指定なし。既存の `package.json` scripts（`dev` / `build` / `preview`）が Vite のデフォルト値（`dev` は5173番、`build` は `dist/`）を前提にしている可能性が高く、CI・Render の設定と食い違わせないため追加のオプションを持ち込んでいない。CI やデプロイ設定でポート番号や outDir を明示する必要が出たらここに追記する
- ファイル冒頭のコメント `// https://vite.dev/config/` は `npm create vite@latest`（react-ts テンプレート）のデフォルト出力に合わせた

## 3. つまずいた点と教訓

- 直前の監査（README）は「Rust backend・DB・API は未配置」としていたが、実際には過去チャットで Task 06（POST /runs）まで back_cargo 配下に実装済みで、Render/Neon へのデプロイも完了していた。原因は「古いチェックアウトに戻した」ことで、実装が失われたわけではない。**監査結果を鵜呑みにせず、まずチェックアウト状態（コミットハッシュ・ブランチ）を確認してから着手する。**
- `package.json` の中身を確認せずに `vite.config.ts` を書くと、`@vitejs/plugin-react` と `-swc` 版のどちらを想定するかで実体が変わり得た。今回は実物を確認してから着手したため手戻りなし。

## 4. 再現コマンド

```bash
cd ~/A/ops-hub

# 復元前の失敗を再現（既に vite.config.ts を配置済みなら成功する）
npm run build

# 復元後
npm run dev       # http://localhost:5173 が開くこと
npm run build     # tsc -b && vite build が通ること
npm run lint      # 引き続き成功すること（影響なし）
```

## 5. 次タスクへの引き継ぎ

- Task 18 で back_cargo を復元、Task 19 で AI 協働フレームワークの残骸を削除、Task 20 で ci.yml に make verify を組み込む
- back_cargo（Rust バックエンド）自体は「未配置」ではなく、古いチェックアウトにより一時的に作業ツリーから外れているだけ。復元が必要なら、過去チャット（Task 02〜06）のソースを再取得するところから
- security-audit.yml の `working-directory: back_cargo` 参照は、back_cargo 復元とセットで解消する
- Dependabot（`.github/dependabot.yml`）・Ruleset 差分（`strict required status checks` / `review thread resolution` / `unattributed changes への追加 approval`）は未着手

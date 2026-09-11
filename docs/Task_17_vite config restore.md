# Task 17: vite.config.ts の復元

## 1. 背景・目的

古いチェックアウト（GitHub `main` 先頭 `511fce6` 相当）に戻したところ、リポジトリ直下に `vite.config.ts` が存在しない状態だった。`tsconfig.node.json` の `include` が `["vite.config.ts"]` のみのため、このファイルが無いと

```
TS18003: No inputs were found in tsconfig.node.json (include: vite.config.ts)
```

で `npm run build`（`tsc -b && vite build`）が失敗する。`npm run dev` は Vite が tsconfig.node.json を経由せず直接起動するため影響を受けず、`npm run lint` も `vite.config.ts` を lint 対象に含めていないため影響を受けない。この非対称性が「lint成功・build失敗」の原因。

品質基盤タスクの1番目として(採番は17。旧07は本来のロードマップのtask-07=stale-lock-recoveryと衝突するため繰り上げ)、これを復元しビルドをgreenに戻す。

## 2. 設計判断

- `package.json` の devDependencies に `@vitejs/plugin-react`（`-swc` 版ではない）のみが入っており、他のVite pluginは入っていない。プラグイン構成は `plugins: [react()]` の最小形とした。
- `tsconfig.app.json` / `tsconfig.node.json` のどちらにも `paths` エイリアス設定が無いため、`resolve.alias`（`@/...` 等）は追加していない。将来エイリアスを導入する場合は `tsconfig.app.json` 側の `paths` とセットで追加する必要がある。
- `server` / `build` 等のオプションは指定なし。既存の `package.json` scripts（`dev` / `build` / `preview`）がViteのデフォルト値（`dev`は5173番、`build`は`dist/`）を前提にしている可能性が高く、CI・Renderの設定と食い違わせないため今回は追加のオプションを持ち込んでいない。次タスクでCIやデプロイ設定を書く際、ポート番号やoutDirを明示する必要が出た場合はここに追記する。
- ファイル冒頭のコメント `// https://vite.dev/config/` は `npm create vite@latest`（react-ts テンプレート）のデフォルト出力に合わせた。

## 3. つまずいた点と教訓

- 直前の監査（README）は「Rust backend・DB・APIは未配置」としていたが、実際には過去チャットでTask 6（POST /runs）までback_cargo配下に実装済みで、Render/Neonへのデプロイも完了していた。原因は今回「古いチェックアウトに戻した」ことによるもので、実装が失われたわけではない。**監査結果を鵜呑みにせず、まずチェックアウト状態（コミットハッシュ・ブランチ）を確認してから着手する**、という教訓。
- `package.json` の中身を確認せずに `vite.config.ts` を書くと、`@vitejs/plugin-react` と `-swc` 版のどちらを想定するかで実体が変わり得た。今回は実物を確認してから着手したため手戻りなし。

## 4. 再現コマンド

```bash
cd ~/A/ops-hub   # 実際のパスに置き換え

# 復元前の失敗を再現(確認用。既にvite.config.tsを配置済みなら成功するはず)
npm run build

# 復元後
npm run dev       # http://localhost:5173 が開くこと
npm run build     # tsc -b && vite build が通ること
npm run lint      # 引き続き成功すること(影響なし)
```

## 5. 次タスクへの引き継ぎ

- タスク18でback_cargoを復元し、タスク19でAI協働フレームワークの残骸を削除、タスク20でci.ymlにmake verifyを組み込む。
- back_cargo（Rustバックエンド）自体は「未配置」ではなく、古いチェックアウトにより一時的に作業ツリーから外れているだけ。復元が必要なら、過去チャット（タスク2〜6）のソースを再取得するところから。
- security-audit.yml の `working-directory: back_cargo` 参照は、back_cargo復元とセットで解消する（back_cargo復元タスクの一部として扱うか、別タスクに切り出すかは着手時に判断）。
- Dependabot（`.github/dependabot.yml`）・Ruleset差分（`strict required status checks` / `review thread resolution` / `unattributed changesへの追加approval`）は未着手のまま。


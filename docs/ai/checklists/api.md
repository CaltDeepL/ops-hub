# HTTP API レビューチェックリスト

対象: route、status code、request/response、error contract。

- [ ] 既存client/monitor workflowとの後方互換性をtaskが明示的に破らない限り維持する。
- [ ] validation failureのHTTP status/bodyがtestで固定されている。
- [ ] duplicate/retry時のresponse semanticsが意図的である。
- [ ] 5xxにsecret/DB/internal detailを出さない。
- [ ] error responseは既存RFC 9457 Problem Details方針に従う。
- [ ] route追加/変更にintegration testがある。
- [ ] auth境界の変更がtaskに明記されている。

## `/v1/runs` 固定事項

- [ ] 正常開始: `202` + `status: started`。
- [ ] lock競合: `200` + `status: already_running`。
- [ ] lock競合時 `run_id: null` を許容する。
- [ ] `recovered: true` は実際にstale lock回収した開始だけ。
- [ ] 409へ変更してGitHub Actions/dead-manを誤失敗させていない。

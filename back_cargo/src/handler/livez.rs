//! liveness 用。**DBに触らない。**
//!
//! プラットフォーム（Render）の再起動判断に使う。プロセスが生きていれば 200 を返す。
//! DB疎通まで見る `/health` とは目的が違う（4章）。

use axum::http::StatusCode;

pub async fn livez() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
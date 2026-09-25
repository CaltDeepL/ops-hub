//! 監視対象への HTTP probe（要件 F-2 / F-3）。
//!
//! ## `probe` は `Result` を返さない
//!
//! 対象が落ちていることは **このシステムにとっての正常系**。`?` で上へ投げると、
//! 1件が落ちているだけで1巡全体が `failed` になり、他の対象の観測結果まで
//! 失われる。失敗は [`CheckResult`] の値として返す。
//!
//! ## リダイレクトを追わない
//!
//! 追うと「301 を返すようになった」変化が `expected_status = 200` のまま success
//! に見え、監視として意味を失う。リダイレクトが正常な対象は
//! `targets.expected_status` に 301 等を設定する。
//!
//! ## ボディを読まない
//!
//! ステータス行とヘッダを受け取った時点で判定する。`duration_ms` は実質 TTFB。
//! コールドスタート検出（F-2）にはこちらが素直に効く。

use std::error::Error as _;
use std::time::{Duration, Instant};

use crate::masking;
use crate::repository::target_repo::Target;

/// `checks.result`（ENUM `check_result`）に書く値。
///
/// Rust 側で `sqlx::Type` を derive しないのは `run_repo` と同じ方針
/// （この層では SQL のリテラルとしてしか扱わない）。書き込み時に
/// [`as_str`](Self::as_str) で `text` として渡し、SQL 側で ENUM へキャストする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    Success,
    Timeout,
    HttpError,
    ConnectionError,
}

impl CheckResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Timeout => "timeout",
            Self::HttpError => "http_error",
            Self::ConnectionError => "connection_error",
        }
    }
}

/// 1回の probe の結果。そのまま `checks` の1行になる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub result: CheckResult,
    pub duration_ms: i32,
    /// 応答を受け取れた場合のみ。
    pub status_code: Option<i32>,
    /// success かつ `degraded_threshold_ms` を超えたとき true。状態遷移には使わない。
    pub degraded: bool,
    /// 失敗時のみ。マスキングと512文字への切り詰めを済ませたもの。
    pub error_detail: Option<String>,
}

/// probe 用の HTTP クライアントを作る。
///
/// タイムアウトは対象ごとに違う（`targets.timeout_ms`）ので、ここでは付けず
/// リクエスト単位で設定する。
pub fn build_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ops-hub/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// 対象に1回リクエストを投げ、結果を分類する。
pub async fn probe(client: &reqwest::Client, target: &Target) -> ProbeOutcome {
    let method = if target.method.eq_ignore_ascii_case("HEAD") {
        reqwest::Method::HEAD
    } else {
        reqwest::Method::GET
    };
    let timeout = Duration::from_millis(u64::try_from(target.timeout_ms).unwrap_or(0));

    let started = Instant::now();
    let response = client
        .request(method, &target.url)
        .timeout(timeout)
        .send()
        .await;
    let duration_ms = elapsed_ms(started);

    match response {
        Ok(response) => {
            let status = i32::from(response.status().as_u16());
            // ボディは読まずに捨てる（モジュール先頭のコメント）
            drop(response);

            if status == target.expected_status {
                ProbeOutcome {
                    result: CheckResult::Success,
                    duration_ms,
                    status_code: storable_status(status),
                    degraded: duration_ms > target.degraded_threshold_ms,
                    error_detail: None,
                }
            } else {
                ProbeOutcome {
                    result: CheckResult::HttpError,
                    duration_ms,
                    status_code: storable_status(status),
                    degraded: false,
                    error_detail: Some(format!(
                        "期待したステータス {} に対して {status} が返りました",
                        target.expected_status
                    )),
                }
            }
        }
        Err(error) => ProbeOutcome {
            result: if error.is_timeout() {
                CheckResult::Timeout
            } else {
                CheckResult::ConnectionError
            },
            duration_ms,
            status_code: None,
            degraded: false,
            error_detail: Some(masking::error_detail(&error_chain(&error))),
        },
    }
}

/// `reqwest::Error` の表示は最上位だけで、`connection refused` などの原因は
/// `source()` の先にある。連鎖をたどって1行にまとめる。
fn error_chain(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        // hyper と reqwest で同じ文言を重ねて返すことがある
        if !message.ends_with(&cause_text) {
            message.push_str(": ");
            message.push_str(&cause_text);
        }
        source = cause.source();
    }
    message
}

/// `checks_status_code_range`（100〜599）に収まらない値は保存しない。
fn storable_status(status: i32) -> Option<i32> {
    (100..=599).contains(&status).then_some(status)
}

fn elapsed_ms(started: Instant) -> i32 {
    i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumの文字列はマイグレーションと一致する() {
        // 0001_init の `check_result` と揃っていないと INSERT のキャストで落ちる
        assert_eq!(CheckResult::Success.as_str(), "success");
        assert_eq!(CheckResult::Timeout.as_str(), "timeout");
        assert_eq!(CheckResult::HttpError.as_str(), "http_error");
        assert_eq!(CheckResult::ConnectionError.as_str(), "connection_error");
    }

    #[test]
    fn 範囲外のステータスは保存しない() {
        assert_eq!(storable_status(200), Some(200));
        assert_eq!(storable_status(99), None);
        assert_eq!(storable_status(600), None);
    }
}

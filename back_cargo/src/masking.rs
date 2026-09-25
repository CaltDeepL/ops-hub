//! 外へ出す文字列から秘匿値を伏せる（N-8 / 詳細設計 4章）。
//!
//! `checks.error_detail` は90日残り、Slack 通知にも載りうる。`reqwest` の
//! エラー表示はリクエスト URL をそのまま含むため、署名付き URL の署名や
//! クエリに載せた API キーがそこへ流れる。
//!
//! ## `config.rs` の `redact_url` と統合しない理由
//!
//! あちらは起動ログの `DATABASE_URL` 専用で、**ユーザ名を残す**（どの資格情報で
//! 繋いでいるか分からないと調査にならない）。こちらは残す理由が無いので
//! ユーザ情報ごと落とす。用途が違う。
//!
//! 方針は**伏せ漏れより伏せ過ぎ**。クエリ文字列は中身を見ずに丸ごと落とす。

/// `checks.error_detail` の上限（文字数）。`checks_error_detail_len` CHECK と揃える。
pub const ERROR_DETAIL_MAX_CHARS: usize = 512;

/// エラー文言を `checks.error_detail` に書ける形にする。
///
/// URL を伏せてから文字数で詰める。順序を逆にすると、詰めた位置で URL が
/// 途中から始まり、スキームを手掛かりにした検出から漏れる。
pub fn error_detail(message: &str) -> String {
    truncate_chars(&mask_urls(message), ERROR_DETAIL_MAX_CHARS)
}

/// 文中の `http://` / `https://` で始まる URL をすべて伏せる。
pub fn mask_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = find_url_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        // エラー表示では URL が `(...)` や引用符で囲まれることが多い
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '>' | '"' | '\'' | ','))
            .unwrap_or(tail.len());
        out.push_str(&mask_url(&tail[..end]));
        rest = &tail[end..];
    }

    out.push_str(rest);
    out
}

/// 1つの URL からユーザ情報・クエリ・フラグメントを落とす。
///
/// クエリがあった事実だけは `?***` で残す。「クエリ付きの URL を叩いていた」
/// ことは調査の手掛かりになるが、中身は要らない。
fn mask_url(url: &str) -> String {
    let Some((scheme, after)) = url.split_once("://") else {
        return url.to_owned();
    };

    let cut = after.find(['?', '#']).unwrap_or(after.len());
    let (body, dropped) = after.split_at(cut);

    let authority_end = body.find('/').unwrap_or(body.len());
    let (authority, path) = body.split_at(authority_end);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);

    let query_marker = if dropped.starts_with('?') { "?***" } else { "" };
    format!("{scheme}://{host}{path}{query_marker}")
}

fn find_url_start(text: &str) -> Option<usize> {
    match (text.find("https://"), text.find("http://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// 文字数で詰める。PostgreSQL の `length()` も文字数なので、バイトで数えない。
fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((byte_index, _)) => text[..byte_index].to_owned(),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn クエリ文字列を丸ごと落とす() {
        assert_eq!(
            mask_urls(
                "error sending request for url (https://api.example.com/health?token=secret&x=1)"
            ),
            "error sending request for url (https://api.example.com/health?***)"
        );
    }

    #[test]
    fn ユーザ情報を落とす() {
        assert_eq!(
            mask_urls("https://user:pass@example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn フラグメントは痕跡を残さず落とす() {
        assert_eq!(
            mask_urls("https://example.com/a#frag"),
            "https://example.com/a"
        );
    }

    #[test]
    fn 複数のurlを伏せる() {
        assert_eq!(
            mask_urls("http://a.example/?k=1 -> https://b.example/?k=2"),
            "http://a.example/?*** -> https://b.example/?***"
        );
    }

    #[test]
    fn urlが無ければそのまま() {
        assert_eq!(mask_urls("connection refused"), "connection refused");
    }

    #[test]
    fn 文字数で詰める() {
        let long = "あ".repeat(ERROR_DETAIL_MAX_CHARS + 10);
        let detail = error_detail(&long);
        assert_eq!(detail.chars().count(), ERROR_DETAIL_MAX_CHARS);
        assert!(detail.ends_with('あ'));
    }

    #[test]
    fn 伏せてから詰める() {
        // クエリが上限を跨いでいても、伏せた後の長さで詰めるので漏れない
        let message = format!("{} https://example.com/?key=secret", "x".repeat(500));
        let detail = error_detail(&message);
        assert!(!detail.contains("secret"), "{detail}");
        assert!(detail.chars().count() <= ERROR_DETAIL_MAX_CHARS);
    }
}

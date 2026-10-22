use serde::Deserialize;
use worker::{Error, Fetch, Headers, Method, Request, RequestInit, Result, Url};

#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    pub ok: bool,
    #[serde(default)]
    pub messages: Vec<Message>,
    pub error: Option<String>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMetadata {
    // 最後のページでは空文字で返ってくる。
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostResponse {
    pub ok: bool,
    pub error: Option<String>,
    pub ts: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthTestResponse {
    pub ok: bool,
    pub error: Option<String>,
    pub url: Option<String>,
}

// chat.getPermalink を 1 件ずつ呼ぶと外部リクエスト数が増えるので、URL は自前で組み立てる。
pub fn permalink(workspace_url: &str, channel: &str, ts: &str) -> String {
    format!(
        "{}/archives/{channel}/p{}",
        workspace_url.trim_end_matches('/'),
        ts.replace('.', "")
    )
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub ts: String,
    pub text: Option<String>,
    pub bot_id: Option<String>,
    pub subtype: Option<String>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

// フィルタに必要なフィールドだけ持てばよい（未知のフィールドは serde が読み捨てる）。
#[derive(Debug, Deserialize)]
pub struct Reaction {
    pub name: String,
}

const API_BASE: &str = "https://slack.com/api";
const PAGE_LIMIT: &str = "200";

fn auth_headers(token: &str) -> Result<Headers> {
    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {token}"))?;
    Ok(headers)
}

// oldest は Unix 秒。これより新しい親メッセージを全ページ分集める。
pub async fn fetch_history(token: &str, channel: &str, oldest: u64) -> Result<Vec<Message>> {
    let mut messages = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = Url::parse(&format!("{API_BASE}/conversations.history"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("channel", channel);
            q.append_pair("oldest", &oldest.to_string());
            q.append_pair("limit", PAGE_LIMIT);
            if let Some(c) = &cursor {
                q.append_pair("cursor", c);
            }
        }

        let mut init = RequestInit::new();
        init.with_method(Method::Get)
            .with_headers(auth_headers(token)?);
        let request = Request::new_with_init(url.as_str(), &init)?;
        let res: HistoryResponse = Fetch::Request(request).send().await?.json().await?;

        if !res.ok {
            return Err(Error::RustError(format!(
                "conversations.history failed: {}",
                res.error.unwrap_or_default()
            )));
        }
        messages.extend(res.messages);

        cursor = res
            .response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|c| !c.is_empty());
        if cursor.is_none() {
            return Ok(messages);
        }
    }
}

// 投稿したメッセージの ts を返す。スレッド返信の親に使う。
pub async fn post_message(
    token: &str,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<String> {
    let headers = auth_headers(token)?;
    headers.set("Content-Type", "application/json; charset=utf-8")?;

    let mut body = serde_json::json!({ "channel": channel, "text": text });
    if let Some(ts) = thread_ts {
        body["thread_ts"] = ts.into();
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.to_string().into()));
    let request = Request::new_with_init(&format!("{API_BASE}/chat.postMessage"), &init)?;
    let res: PostResponse = Fetch::Request(request).send().await?.json().await?;

    match (res.ok, res.ts) {
        (true, Some(ts)) => Ok(ts),
        _ => Err(Error::RustError(format!(
            "chat.postMessage failed: {}",
            res.error.unwrap_or_default()
        ))),
    }
}

// ワークスペースの URL（https://xxx.slack.com/）を返す。
pub async fn workspace_url(token: &str) -> Result<String> {
    let mut init = RequestInit::new();
    init.with_method(Method::Get)
        .with_headers(auth_headers(token)?);
    let request = Request::new_with_init(&format!("{API_BASE}/auth.test"), &init)?;
    let res: AuthTestResponse = Fetch::Request(request).send().await?.json().await?;

    match (res.ok, res.url) {
        (true, Some(url)) => Ok(url),
        _ => Err(Error::RustError(format!(
            "auth.test failed: {}",
            res.error.unwrap_or_default()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_permalink_from_ts() {
        assert_eq!(
            permalink("https://example.slack.com/", "C123", "1700000000.000100"),
            "https://example.slack.com/archives/C123/p1700000000000100"
        );
    }

    #[test]
    fn parses_message_with_reactions() {
        let json = r#"{
            "ok": true,
            "messages": [
                {"ts": "1700000000.000100", "text": "buy milk",
                 "reactions": [{"name": "done", "count": 1, "users": ["U1"]}]}
            ]
        }"#;
        let res: HistoryResponse = serde_json::from_str(json).unwrap();
        assert!(res.ok);
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].reactions.len(), 1);
    }

    #[test]
    fn parses_message_without_reactions() {
        let json = r#"{
            "ok": true,
            "messages": [
                {"ts": "1700000000.000100", "text": "buy milk"}
            ]
        }"#;
        let res: HistoryResponse = serde_json::from_str(json).unwrap();
        assert!(res.ok);
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].reactions.len(), 0);
    }
}

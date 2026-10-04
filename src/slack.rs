use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    pub ok: bool,
    #[serde(default)]
    pub messages: Vec<Message>,
    pub error: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

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

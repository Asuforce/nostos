use crate::slack::Message;

pub fn unfinished_tasks<'a>(messages: &'a [Message], done_reaction: &str) -> Vec<&'a Message> {
    messages
        .iter()
        .filter(|m| is_unfinished(m, done_reaction))
        .collect()
}

pub fn format_header(count: usize) -> String {
    format!("おはようございます！未完了のTodoが{count}件あります。スレッドにリンクを貼ります。")
}

// Slack は 1 メッセージ 4000 文字を超えると切り詰めるので、行単位で分割する。
const CHUNK_LIMIT: usize = 3000;
const LABEL_LIMIT: usize = 40;

pub fn format_thread_replies(tasks: &[&Message], link: impl Fn(&Message) -> String) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for task in tasks {
        let line = format!("・<{}|{}>", link(task), label(task));
        if !current.is_empty() && current.len() + line.len() + 1 > CHUNK_LIMIT {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(&line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

// リンクの表示名。mrkdwn の制御文字をエスケープしないと <url|label> が壊れる。
fn label(message: &Message) -> String {
    let text = message.text.as_deref().unwrap_or_default();
    let first_line = text.lines().next().unwrap_or_default();
    let truncated: String = first_line.chars().take(LABEL_LIMIT).collect();
    truncated
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "¦")
}

// bot_id / subtype のある投稿を除外しないと、リマインダー自身の投稿を拾い続ける。
fn is_unfinished(message: &Message, done_reaction: &str) -> bool {
    message.bot_id.is_none()
        && message.subtype.is_none()
        && !message.reactions.iter().any(|r| r.name == done_reaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(json: &str) -> Message {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn keeps_plain_user_message() {
        let messages = vec![msg(r#"{"ts": "1", "text": "buy milk"}"#)];
        assert_eq!(unfinished_tasks(&messages, "done").len(), 1);
    }

    #[test]
    fn excludes_done_message() {
        let messages = vec![msg(
            r#"{"ts": "1", "text": "x", "reactions": [{"name": "done", "count": 1}]}"#,
        )];
        assert_eq!(unfinished_tasks(&messages, "done").len(), 0);
    }

    #[test]
    fn left_without_done() {
        let messages = vec![msg(
            r#"{"ts": "1", "text": "x", "reactions": [{"name": "eyes", "count": 1}]}"#,
        )];
        assert_eq!(unfinished_tasks(&messages, "done").len(), 1);
    }

    #[test]
    fn formats_header_with_count() {
        assert_eq!(
            format_header(2),
            "おはようございます！未完了のTodoが2件あります。スレッドにリンクを貼ります。"
        );
    }

    #[test]
    fn formats_one_link_line_per_task() {
        let messages = vec![
            msg(r#"{"ts": "1", "text": "buy milk"}"#),
            msg(r#"{"ts": "2", "text": "call <mom> & dad"}"#),
        ];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, |m| format!("https://x/p{}", m.ts));
        assert_eq!(
            replies,
            vec!["・<https://x/p1|buy milk>\n・<https://x/p2|call &lt;mom&gt; &amp; dad>"]
        );
    }

    #[test]
    fn splits_replies_when_too_long() {
        let messages: Vec<Message> = (0..100)
            .map(|i| msg(&format!(r#"{{"ts": "{i}", "text": "task {i}"}}"#)))
            .collect();
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, |m| {
            format!("https://example.slack.com/archives/C1/p{}", m.ts)
        });
        assert!(replies.len() > 1);
        assert!(replies.iter().all(|r| r.len() <= CHUNK_LIMIT));
        assert_eq!(
            replies.iter().map(|r| r.lines().count()).sum::<usize>(),
            100
        );
    }

    #[test]
    fn honors_custom_done_reaction() {
        let messages = vec![msg(
            r#"{"ts": "1", "text": "x", "reactions": [{"name": "white_check_mark", "count": 1}]}"#,
        )];
        assert!(unfinished_tasks(&messages, "white_check_mark").is_empty());
        assert_eq!(unfinished_tasks(&messages, "done").len(), 1);
    }

    #[test]
    fn excludes_bot_message() {
        let messages = vec![msg(r#"{"ts": "1", "text": "x", "bot_id": "B1"}"#)];
        assert!(unfinished_tasks(&messages, "done").is_empty());
    }

    #[test]
    fn excludes_subtype_message() {
        let messages = vec![msg(
            r#"{"ts": "1", "text": "x", "subtype": "channel_join"}"#,
        )];
        assert!(unfinished_tasks(&messages, "done").is_empty());
    }
}

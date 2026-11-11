use crate::slack::Message;

pub fn unfinished_tasks<'a>(messages: &'a [Message], done_reaction: &str) -> Vec<&'a Message> {
    // ":done:" と書かれても動くよう、前後のコロンは取り除く。
    let done_reaction = done_reaction.trim_matches(':');
    let mut tasks: Vec<&Message> = messages
        .iter()
        .filter(|m| is_unfinished(m, done_reaction))
        .collect();
    // sort_by_key は安定ソートなので、同じ priority の中では元の順序が保たれる。
    tasks.sort_by_key(|m| std::cmp::Reverse(priority(m)));
    tasks
}

// :one: 〜 :five:（またはカスタム絵文字の :1: 〜 :5:）を priority 1〜5 として読む。
// 複数付いていたら最大値、なければ 0。
pub fn priority(message: &Message) -> u8 {
    message
        .reactions
        .iter()
        .filter_map(|r| match r.name.as_str() {
            "one" | "1" => Some(1),
            "two" | "2" => Some(2),
            "three" | "3" => Some(3),
            "four" | "4" => Some(4),
            "five" | "5" => Some(5),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

pub fn format_header(count: usize) -> String {
    format!("Todo: {count}件")
}

// Slack は 1 メッセージ 4000 文字を超えると切り詰めるので、行単位で分割する。
const CHUNK_LIMIT: usize = 3000;
const LABEL_LIMIT: usize = 40;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

// 投稿が保持期限を過ぎて見えなくなるまでの残り日数。
//
// 期限は投稿時刻（ts の秒部分）＋保持日数で、残りは切り上げる。
// 投稿直後は保持日数そのものになり、期限を過ぎたら 0。
// 時計のずれで ts が未来でも保持日数を超えないよう丸める。
// ts が数値として読めない場合は None（表示を付けない）。
pub fn remaining_days(message: &Message, now_secs: u64, retention_days: u64) -> Option<u64> {
    let posted_secs: u64 = message.ts.split('.').next()?.parse().ok()?;
    let expiry_secs = posted_secs.saturating_add(retention_days * SECONDS_PER_DAY);
    if expiry_secs <= now_secs {
        return Some(0);
    }
    let remaining_secs = expiry_secs - now_secs;
    Some(remaining_secs.div_ceil(SECONDS_PER_DAY).min(retention_days))
}

// 残り日数の表示。0 日は「今日消えます」として目立たせる。
fn remaining_mark(message: &Message, now_secs: u64, retention_days: u64) -> String {
    match remaining_days(message, now_secs, retention_days) {
        Some(0) => "（今日消えます）".to_string(),
        Some(days) => format!("（あと{days}日）"),
        None => String::new(),
    }
}

pub fn format_thread_replies(
    tasks: &[&Message],
    now_secs: u64,
    retention_days: u64,
    link: impl Fn(&Message) -> String,
) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for task in tasks {
        let line = format!(
            "・{}<{}|{}>{}",
            priority_mark(task),
            link(task),
            label(task),
            remaining_mark(task, now_secs, retention_days)
        );
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

// priority のある todo だけ行頭に【n】を付ける。
fn priority_mark(message: &Message) -> String {
    match priority(message) {
        0 => String::new(),
        n => format!("【{n}】"),
    }
}

// リンクの表示名。mrkdwn の制御文字をエスケープしないと <url|label> が壊れる。
fn label(message: &Message) -> String {
    let text = message.text.as_deref().unwrap_or_default();
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("(本文なし)");
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
        assert_eq!(format_header(2), "Todo: 2件");
    }

    #[test]
    fn formats_one_link_line_per_task() {
        let messages = vec![
            msg(r#"{"ts": "1", "text": "buy milk"}"#),
            msg(r#"{"ts": "2", "text": "call <mom> & dad"}"#),
        ];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1, 90, |m| format!("https://x/p{}", m.ts));
        assert_eq!(
            replies,
            vec!["・<https://x/p1|buy milk>（あと90日）\n・<https://x/p2|call &lt;mom&gt; &amp; dad>（あと90日）"]
        );
    }

    #[test]
    fn labels_with_first_non_empty_line() {
        let messages = vec![msg(r#"{"ts": "1", "text": "\n  \nbuy milk\nmore"}"#)];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1, 90, |_| "https://x".to_string());
        assert_eq!(replies, vec!["・<https://x|buy milk>（あと90日）"]);
    }

    #[test]
    fn splits_replies_when_too_long() {
        let messages: Vec<Message> = (0..100)
            .map(|i| msg(&format!(r#"{{"ts": "{i}", "text": "task {i}"}}"#)))
            .collect();
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1, 90, |m| {
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
    fn accepts_done_reaction_with_colons() {
        let messages = vec![msg(
            r#"{"ts": "1", "text": "x", "reactions": [{"name": "done", "count": 1}]}"#,
        )];
        assert!(unfinished_tasks(&messages, ":done:").is_empty());
    }

    fn with_reactions(ts: &str, names: &[&str]) -> Message {
        let reactions: Vec<String> = names
            .iter()
            .map(|n| format!(r#"{{"name": "{n}", "count": 1}}"#))
            .collect();
        msg(&format!(
            r#"{{"ts": "{ts}", "text": "t{ts}", "reactions": [{}]}}"#,
            reactions.join(",")
        ))
    }

    #[test]
    fn marks_priority_in_reply_line() {
        let messages = vec![with_reactions("1", &["three"]), with_reactions("2", &[])];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1, 90, |m| format!("https://x/p{}", m.ts));
        assert_eq!(
            replies,
            vec!["・【3】<https://x/p1|t1>（あと90日）\n・<https://x/p2|t2>（あと90日）"]
        );
    }

    #[test]
    fn priority_is_zero_without_number_reaction() {
        assert_eq!(priority(&with_reactions("1", &[])), 0);
        assert_eq!(priority(&with_reactions("1", &["eyes"])), 0);
    }

    #[test]
    fn priority_reads_number_emoji_names() {
        for (name, expected) in [
            ("one", 1),
            ("two", 2),
            ("three", 3),
            ("four", 4),
            ("five", 5),
        ] {
            assert_eq!(priority(&with_reactions("1", &[name])), expected, "{name}");
        }
    }

    #[test]
    fn priority_reads_custom_digit_names() {
        for n in 1..=5u8 {
            assert_eq!(priority(&with_reactions("1", &[&n.to_string()])), n);
        }
    }

    #[test]
    fn priority_ignores_out_of_range_numbers() {
        assert_eq!(
            priority(&with_reactions("1", &["0", "6", "zero", "six"])),
            0
        );
    }

    #[test]
    fn priority_takes_highest_when_multiple() {
        assert_eq!(priority(&with_reactions("1", &["two", "eyes", "four"])), 4);
    }

    #[test]
    fn sorts_by_priority_descending() {
        let messages = vec![
            with_reactions("1", &[]),
            with_reactions("2", &["two"]),
            with_reactions("3", &["five"]),
            with_reactions("4", &["one"]),
        ];
        let tasks = unfinished_tasks(&messages, "done");
        let order: Vec<&str> = tasks.iter().map(|m| m.ts.as_str()).collect();
        assert_eq!(order, vec!["3", "2", "4", "1"]);
    }

    #[test]
    fn keeps_original_order_within_same_priority() {
        let messages = vec![
            with_reactions("1", &["three"]),
            with_reactions("2", &[]),
            with_reactions("3", &["three"]),
            with_reactions("4", &[]),
        ];
        let tasks = unfinished_tasks(&messages, "done");
        let order: Vec<&str> = tasks.iter().map(|m| m.ts.as_str()).collect();
        assert_eq!(order, vec!["1", "3", "2", "4"]);
    }

    #[test]
    fn done_wins_over_priority() {
        let messages = vec![with_reactions("1", &["five", "done"])];
        assert!(unfinished_tasks(&messages, "done").is_empty());
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

    #[test]
    fn remaining_days_is_full_retention_when_just_posted() {
        let m = msg(r#"{"ts": "1700000000.000100", "text": "x"}"#);
        assert_eq!(remaining_days(&m, 1_700_000_000, 90), Some(90));
        // 1 秒後も切り上げで 90 のまま。
        assert_eq!(remaining_days(&m, 1_700_000_001, 90), Some(90));
        // 丸 1 日後は 89。
        assert_eq!(remaining_days(&m, 1_700_000_000 + 86_400, 90), Some(89));
    }

    #[test]
    fn remaining_days_counts_down_to_zero_at_expiry() {
        let m = msg(r#"{"ts": "1700000000.000100", "text": "x"}"#);
        let day = 86_400;
        assert_eq!(remaining_days(&m, 1_700_000_000 + 89 * day, 90), Some(1));
        assert_eq!(
            remaining_days(&m, 1_700_000_000 + 90 * day - 1, 90),
            Some(1)
        );
        assert_eq!(remaining_days(&m, 1_700_000_000 + 90 * day, 90), Some(0));
        assert_eq!(remaining_days(&m, 1_700_000_000 + 91 * day, 90), Some(0));
    }

    #[test]
    fn remaining_days_never_exceeds_retention_for_future_ts() {
        let m = msg(r#"{"ts": "1700000100.000100", "text": "x"}"#);
        assert_eq!(remaining_days(&m, 1_700_000_000, 90), Some(90));
    }

    #[test]
    fn remaining_days_honors_custom_retention() {
        let m = msg(r#"{"ts": "1700000000.000100", "text": "x"}"#);
        assert_eq!(remaining_days(&m, 1_700_000_000, 30), Some(30));
    }

    #[test]
    fn remaining_days_is_none_for_unparseable_ts() {
        let m = msg(r#"{"ts": "not-a-ts", "text": "x"}"#);
        assert_eq!(remaining_days(&m, 1_700_000_000, 90), None);
    }

    #[test]
    fn marks_expiring_today_in_reply_line() {
        let messages = vec![msg(r#"{"ts": "1700000000.000100", "text": "buy milk"}"#)];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1_700_000_000 + 90 * 86_400, 90, |_| {
            "https://x".to_string()
        });
        assert_eq!(replies, vec!["・<https://x|buy milk>（今日消えます）"]);
    }
}

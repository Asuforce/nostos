use std::collections::HashSet;

use crate::slack::Message;

const COMPLETED_HEADER: &str = "前回以降の完了:";

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

pub fn format_header(unfinished_count: usize, completed_count: usize) -> String {
    if completed_count == 0 {
        format!("Todo: {unfinished_count}件")
    } else {
        format!("Todo: {unfinished_count}件（前回以降の完了: {completed_count}件）")
    }
}

// Slack は 1 メッセージ 4000 文字を超えると切り詰めるので、行単位で分割する。
const CHUNK_LIMIT: usize = 3000;
const LABEL_LIMIT: usize = 40;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

fn parse_ts_secs(ts: &str) -> Option<u64> {
    ts.split('.').next()?.parse().ok()
}

fn parse_ts_key(ts: &str) -> Option<(u64, u64)> {
    let mut parts = ts.split('.');
    let secs = parts.next()?.parse().ok()?;
    let micros = parts.next().unwrap_or("0").parse().ok()?;
    Some((secs, micros))
}

fn ts_id(ts: &str) -> String {
    ts.replace('.', "")
}

// 投稿が保持期限を過ぎて見えなくなるまでの残り日数。
//
// 期限は投稿時刻（ts の秒部分）＋保持日数で、残りは切り上げる。
// 投稿直後は保持日数そのものになり、期限を過ぎたら 0。
// 時計のずれで ts が未来でも保持日数を超えないよう丸める。
// ts が数値として読めない場合は None（表示を付けない）。
pub fn remaining_days(message: &Message, now_secs: u64, retention_days: u64) -> Option<u64> {
    let posted_secs = parse_ts_secs(&message.ts)?;
    let expiry_secs = posted_secs.saturating_add(retention_days * SECONDS_PER_DAY);
    if expiry_secs <= now_secs {
        return Some(0);
    }
    let remaining_secs = expiry_secs - now_secs;
    Some(remaining_secs.div_ceil(SECONDS_PER_DAY).min(retention_days))
}

// 手動指定と保持期限による優先度の高い方を使う。
pub fn effective_priority(message: &Message, now_secs: u64, retention_days: u64) -> u8 {
    let automatic = match remaining_days(message, now_secs, retention_days) {
        Some(days @ 0..=50) => (1 + (50 - days) / 10).min(5) as u8,
        _ => 0,
    };
    priority(message).max(automatic)
}

// JST の暦日番号。JST は UTC+9 で夏時間がないため、オフセットを足して日割りする。
fn jst_day(secs: u64) -> u64 {
    (secs + 9 * 60 * 60) / SECONDS_PER_DAY
}

// 残り日数の表示。
//
// 消える日（JST）が今日のものは「（今日まで）」とする。期限ちょうどや
// 期限超過（残り 0 日）も同じ表示にする。切り上げでは残り 1 日になる投稿でも、
// 今日中に消えるものは翌朝には取得範囲外になってしまうため、秒単位の残りでは
// なく暦日で判定するのが最終通知として正確になる。
fn remaining_mark(message: &Message, now_secs: u64, retention_days: u64) -> String {
    let Some(days) = remaining_days(message, now_secs, retention_days) else {
        return String::new();
    };
    if days == 0 {
        return "（今日まで）".to_string();
    }
    let Some(posted_secs) = parse_ts_secs(&message.ts) else {
        return String::new();
    };
    let expiry_secs = posted_secs.saturating_add(retention_days * SECONDS_PER_DAY);
    if jst_day(expiry_secs) == jst_day(now_secs) {
        "（今日まで）".to_string()
    } else {
        format!("（{days}日）")
    }
}

fn chunk_lines(lines: &[String], header: Option<&str>) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = header.unwrap_or_default().to_string();
    let mut has_item = false;

    for line in lines {
        if has_item && current.len() + line.len() + 1 > CHUNK_LIMIT {
            chunks.push(current);
            current = header.unwrap_or_default().to_string();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        has_item = true;
    }
    if has_item {
        chunks.push(current);
    }
    chunks
}

pub fn format_thread_replies(
    tasks: &[&Message],
    now_secs: u64,
    retention_days: u64,
    link: impl Fn(&Message) -> String,
) -> Vec<String> {
    let mut tasks = tasks.to_vec();
    tasks.sort_by_key(|m| std::cmp::Reverse(effective_priority(m, now_secs, retention_days)));
    let lines: Vec<String> = tasks
        .iter()
        .map(|task| {
            format!(
                "・{}<{}|{}>{}",
                priority_mark(task, now_secs, retention_days),
                link(task),
                label(task),
                remaining_mark(task, now_secs, retention_days)
            )
        })
        .collect();
    chunk_lines(&lines, None)
}

pub fn format_completed_replies(
    tasks: &[&Message],
    link: impl Fn(&Message) -> String,
) -> Vec<String> {
    let lines: Vec<String> = tasks
        .iter()
        .map(|task| format!("・<{}|{}>", link(task), label(task)))
        .collect();
    chunk_lines(&lines, Some(COMPLETED_HEADER))
}

// 今日より前（JST）に投稿された最新の Nostos リマインド親メッセージを探す。
pub fn find_previous_reminder(messages: &[Message], now_secs: u64) -> Option<&Message> {
    let today = jst_day(now_secs);
    messages
        .iter()
        .filter(|m| {
            m.bot_id.is_some()
                && m.text.as_deref().is_some_and(|t| t.starts_with("Todo: "))
                && parse_ts_secs(&m.ts).is_some_and(|secs| jst_day(secs) < today)
        })
        .max_by_key(|m| parse_ts_key(&m.ts))
}

// 前回の Nostos スレッド返信から未完了タスクの ts ID（ドット除去済み）を抽出する。
// 完了リストの返信は誤認を防ぐため除外する。
pub fn extract_linked_ts_ids(
    replies: &[Message],
    reminder_ts: &str,
    channel: &str,
) -> HashSet<String> {
    let marker = format!("/archives/{channel}/p");
    let mut ids = HashSet::new();

    for reply in replies {
        if reply.ts == reminder_ts {
            continue;
        }
        let Some(text) = reply.text.as_deref() else {
            continue;
        };
        if text.starts_with(COMPLETED_HEADER) || text.starts_with("昨日完了:") {
            continue;
        }
        let mut rest = text;
        while let Some(pos) = rest.find(&marker) {
            let after = &rest[pos + marker.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                ids.insert(digits);
            }
            rest = after;
        }
    }
    ids
}

// 前回のリマインド時点で未完了だったもの、または前回以降に投稿されて完了したものを抽出する。
pub fn completed_since_previous<'a>(
    messages: &'a [Message],
    done_reaction: &str,
    prev_reminder_ts: Option<&str>,
    prev_unfinished_ids: &HashSet<String>,
    now_secs: u64,
) -> Vec<&'a Message> {
    let done_reaction = done_reaction.trim_matches(':');
    let prev_key = prev_reminder_ts.and_then(parse_ts_key);
    let today = jst_day(now_secs);

    messages
        .iter()
        .filter(|m| {
            if !is_user_message(m) || !has_done_reaction(m, done_reaction) {
                return false;
            }
            match prev_key {
                Some(pk) => {
                    prev_unfinished_ids.contains(&ts_id(&m.ts))
                        || parse_ts_key(&m.ts).is_some_and(|k| k > pk)
                }
                None => parse_ts_secs(&m.ts).is_some_and(|secs| jst_day(secs) + 1 == today),
            }
        })
        .collect()
}

// priority のある todo だけ行頭に前後を半角スペースで区切った絵文字を付ける（5 は :fire:）。
fn priority_mark(message: &Message, now_secs: u64, retention_days: u64) -> String {
    match effective_priority(message, now_secs, retention_days) {
        1 => " :one: ".to_string(),
        2 => " :two: ".to_string(),
        3 => " :three: ".to_string(),
        4 => " :four: ".to_string(),
        5 => " :fire: ".to_string(),
        _ => String::new(),
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

fn is_user_message(message: &Message) -> bool {
    message.bot_id.is_none() && message.subtype.is_none()
}

fn has_done_reaction(message: &Message, done_reaction: &str) -> bool {
    message.reactions.iter().any(|r| r.name == done_reaction)
}

// bot_id / subtype のある投稿を除外しないと、リマインダー自身の投稿を拾い続ける。
fn is_unfinished(message: &Message, done_reaction: &str) -> bool {
    is_user_message(message) && !has_done_reaction(message, done_reaction)
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
        assert_eq!(format_header(2, 0), "Todo: 2件");
        assert_eq!(format_header(2, 3), "Todo: 2件（前回以降の完了: 3件）");
        assert_eq!(format_header(0, 1), "Todo: 0件（前回以降の完了: 1件）");
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
            vec!["・<https://x/p1|buy milk>（90日）\n・<https://x/p2|call &lt;mom&gt; &amp; dad>（90日）"]
        );
    }

    #[test]
    fn labels_with_first_non_empty_line() {
        let messages = vec![msg(r#"{"ts": "1", "text": "\n  \nbuy milk\nmore"}"#)];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1, 90, |_| "https://x".to_string());
        assert_eq!(replies, vec!["・<https://x|buy milk>（90日）"]);
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
            vec!["・ :three: <https://x/p1|t1>（90日）\n・<https://x/p2|t2>（90日）"]
        );
    }

    #[test]
    fn surrounds_all_priority_stamps_with_spaces() {
        for (n, name) in [
            (1, "one"),
            (2, "two"),
            (3, "three"),
            (4, "four"),
            (5, "fire"),
        ] {
            let message = with_reactions("1", &[&n.to_string()]);
            let replies = format_thread_replies(&[&message], 1, 90, |_| "https://x".to_string());
            assert_eq!(replies, vec![format!("・ :{name}: <https://x|t1>（90日）")]);
        }
        assert_eq!(priority_mark(&with_reactions("1", &[]), 1, 90), "");
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
    fn raises_priority_at_ten_day_boundaries() {
        let m = with_reactions("1700000000", &[]);
        for (days, expected) in [
            (51, 0),
            (50, 1),
            (41, 1),
            (40, 2),
            (30, 3),
            (20, 4),
            (10, 5),
            (0, 5),
        ] {
            let now = 1_700_000_000 + (90 - days) * SECONDS_PER_DAY;
            assert_eq!(effective_priority(&m, now, 90), expected, "{days}");
        }
        let manual = with_reactions("1700000000", &["five"]);
        assert_eq!(effective_priority(&manual, 1_700_000_000, 90), 5);
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
        assert_eq!(replies, vec!["・ :fire: <https://x|buy milk>（今日まで）"]);
    }

    #[test]
    fn marks_as_due_today_when_expiring_later_today_jst() {
        // now は JST 09:00、期限は同日 18:00 (JST)。秒単位の残りは 9 時間で
        // 切り上げると 1 日になるが、暦日が今日なので「今日まで」になる。
        let messages = vec![msg(r#"{"ts": "1692349200.000100", "text": "buy milk"}"#)];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1_700_092_800, 90, |_| "https://x".to_string());
        assert_eq!(replies, vec!["・ :fire: <https://x|buy milk>（今日まで）"]);
    }

    #[test]
    fn marks_one_day_left_when_expiring_tomorrow_jst() {
        // now は JST 09:00、期限は翌日 08:00 (JST)。残りは 23 時間で切り上げ
        // 1 日、暦日も翌日なので「1日」になる。
        let messages = vec![msg(r#"{"ts": "1692399600.000100", "text": "buy milk"}"#)];
        let tasks = unfinished_tasks(&messages, "done");
        let replies = format_thread_replies(&tasks, 1_700_092_800, 90, |_| "https://x".to_string());
        assert_eq!(replies, vec!["・ :fire: <https://x|buy milk>（1日）"]);
    }

    #[test]
    fn finds_latest_previous_day_reminder() {
        // now: 2023-11-16 09:00 JST = 1700092800
        let now = 1_700_092_800;
        let messages = vec![
            // 同日 08:00 JST (1700089200) のリマインドは除外される。
            msg(r#"{"ts": "1700089200.000100", "text": "Todo: 1件", "bot_id": "B1"}"#),
            // 昨日 09:00 JST (1700006400) のリマインドが選ばれる。
            msg(r#"{"ts": "1700006400.000100", "text": "Todo: 2件", "bot_id": "B1"}"#),
            // 一昨日 09:00 JST (1699920000) のリマインドより新しいものが優先。
            msg(r#"{"ts": "1699920000.000100", "text": "Todo: 3件", "bot_id": "B1"}"#),
            // ユーザー投稿や別フォーマットのボット投稿は無視する。
            msg(r#"{"ts": "1700050000.000100", "text": "Todo: 99件"}"#),
            msg(r#"{"ts": "1700050001.000100", "text": "hello", "bot_id": "B2"}"#),
        ];
        let prev = find_previous_reminder(&messages, now).unwrap();
        assert_eq!(prev.ts, "1700006400.000100");
    }

    #[test]
    fn extracts_linked_ts_ids_excluding_parent_and_completed_section() {
        let replies = vec![
            msg(
                r#"{"ts": "1700006400.000100", "text": "Todo: 2件（前回以降の完了: 1件）", "bot_id": "B1"}"#,
            ),
            msg(
                r#"{"ts": "1700006401.000100", "text": "・ :three: <https://example.slack.com/archives/C123/p1699900000000100|old task>（80日）\n・<https://example.slack.com/archives/C123/p1699950000000200|task 2>（89日）", "bot_id": "B1"}"#,
            ),
            msg(
                r#"{"ts": "1700006402.000100", "text": "前回以降の完了:\n・<https://example.slack.com/archives/C123/p1699800000000300|done yesterday>", "bot_id": "B1"}"#,
            ),
        ];
        let ids = extract_linked_ts_ids(&replies, "1700006400.000100", "C123");
        let expected: HashSet<String> = ["1699900000000100", "1699950000000200"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn collects_completed_tasks_since_previous_reminder() {
        // 前回リマインド: 1700006400.000100
        // now: 1700092800
        let prev_ts = "1700006400.000100";
        let prev_ids: HashSet<String> = ["1699000000000100".to_string()].into_iter().collect();
        let messages = vec![
            // 前回未完了リストに載っていて、今回 done になった古いタスク -> 対象
            msg(
                r#"{"ts": "1699000000.000100", "text": "carried over", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
            // 前回以前からすでに done だった古いタスク（前回リスト外） -> 対象外
            msg(
                r#"{"ts": "1698000000.000100", "text": "already done long ago", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
            // 前回リマインド以降に投稿され、すでに done になったタスク -> 対象
            msg(
                r#"{"ts": "1700050000.000100", "text": "created and done yesterday", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
            // まだ未完了 -> 対象外
            msg(r#"{"ts": "1700060000.000100", "text": "still open"}"#),
            // bot 投稿 -> 対象外
            msg(
                r#"{"ts": "1700070000.000100", "text": "bot msg", "bot_id": "B1", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
        ];

        let completed =
            completed_since_previous(&messages, ":done:", Some(prev_ts), &prev_ids, 1_700_092_800);
        let ts_list: Vec<&str> = completed.iter().map(|m| m.ts.as_str()).collect();
        assert_eq!(ts_list, vec!["1699000000.000100", "1700050000.000100"]);
    }

    #[test]
    fn collects_previous_day_completed_when_no_previous_reminder() {
        // now: 2023-11-16 09:00 JST (1700092800)
        // yesterday JST: 2023-11-15 00:00..24:00 JST (1699974000..1700060400)
        let messages = vec![
            msg(
                r#"{"ts": "1699973999.000100", "text": "two days ago", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
            msg(
                r#"{"ts": "1700000000.000100", "text": "yesterday", "reactions": [{"name": "done", "count": 1}]}"#,
            ),
        ];
        let completed =
            completed_since_previous(&messages, "done", None, &HashSet::new(), 1_700_092_800);
        let ts_list: Vec<&str> = completed.iter().map(|m| m.ts.as_str()).collect();
        assert_eq!(ts_list, vec!["1700000000.000100"]);
    }

    #[test]
    fn formats_completed_replies_with_header_and_splits_when_long() {
        let messages: Vec<Message> = (0..100)
            .map(|i| msg(&format!(r#"{{"ts": "{i}", "text": "done task {i}"}}"#)))
            .collect();
        let refs: Vec<&Message> = messages.iter().collect();
        let replies = format_completed_replies(&refs, |m| {
            format!("https://example.slack.com/archives/C1/p{}", m.ts)
        });
        assert!(replies.len() > 1);
        assert!(replies.iter().all(|r| r.len() <= CHUNK_LIMIT));
        assert!(replies.iter().all(|r| r.starts_with("前回以降の完了:\n")));
    }
}

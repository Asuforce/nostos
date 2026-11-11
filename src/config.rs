// Slack 無料版の閲覧制限（既定値）の日数。`RETENTION_DAYS` 未設定時はこれを使う。
pub const DEFAULT_RETENTION_DAYS: u64 = 90;

// `RETENTION_DAYS` の値を保持日数として読む。
//
// 未設定・空・数値でない・0 の場合は既定値に倒す。0 日を許すと全タスクが
// 即日期限切れになり、リマインドとして意味をなさないため。
pub fn parse_retention_days(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&days| days > 0)
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}

// `SLACK_CHANNEL_ID` の値をチャンネル ID の一覧に分解する。
//
// 後方互換のため変数名は単数のまま、カンマ区切りで複数を書けるようにする。
// 既存の単一チャンネルの設定はそのまま動く。
// 空の要素は捨て、重複は最初の出現だけを残す（同じチャンネルへの二重投稿を防ぐ）。
pub fn parse_channel_ids(raw: &str) -> Vec<String> {
    let mut channels: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let id = part.trim();
        if !id.is_empty() && !channels.iter().any(|c| c == id) {
            channels.push(id.to_string());
        }
    }
    channels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_channel() {
        assert_eq!(parse_channel_ids("C123"), vec!["C123"]);
    }

    #[test]
    fn parses_multiple_channels() {
        assert_eq!(parse_channel_ids("C123,C456"), vec!["C123", "C456"]);
    }

    #[test]
    fn trims_spaces_and_skips_empty_parts() {
        assert_eq!(parse_channel_ids(" C123 , , C456 ,"), vec!["C123", "C456"]);
    }

    #[test]
    fn deduplicates_channels() {
        assert_eq!(parse_channel_ids("C123,C456,C123"), vec!["C123", "C456"]);
    }

    #[test]
    fn returns_empty_for_blank_input() {
        assert!(parse_channel_ids("").is_empty());
        assert!(parse_channel_ids(" , ").is_empty());
    }

    #[test]
    fn retention_defaults_when_unset_or_invalid() {
        assert_eq!(parse_retention_days(None), 90);
        assert_eq!(parse_retention_days(Some("")), 90);
        assert_eq!(parse_retention_days(Some("abc")), 90);
        assert_eq!(parse_retention_days(Some("0")), 90);
        assert_eq!(parse_retention_days(Some("-5")), 90);
    }

    #[test]
    fn retention_parses_positive_days() {
        assert_eq!(parse_retention_days(Some("90")), 90);
        assert_eq!(parse_retention_days(Some("30")), 30);
        assert_eq!(parse_retention_days(Some(" 45 ")), 45);
    }
}

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
}

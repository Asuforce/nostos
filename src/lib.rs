use worker::*;

pub mod config;
pub mod filter;
pub mod slack;

const DEFAULT_DONE_REACTION: &str = "done";

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Err(e) = run(&env).await {
        console_error!("nostos: {e}");
    }
}

async fn run(env: &Env) -> Result<()> {
    let token = env.secret("SLACK_BOT_TOKEN")?.to_string();
    let channels = config::parse_channel_ids(&env.secret("SLACK_CHANNEL_ID")?.to_string());
    if channels.is_empty() {
        return Err(Error::RustError(
            "SLACK_CHANNEL_ID does not contain a channel ID".to_string(),
        ));
    }

    // リアクション名は秘密情報ではないので、未設定なら既定値を使う。
    let done_reaction = env
        .var("DONE_REACTION")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| DEFAULT_DONE_REACTION.to_string());

    // 保持日数は秘密情報ではないので、未設定なら既定値（90 日）を使う。
    // 取得範囲も同じ日数に揃える。保持期間を過ぎた投稿は Slack 側でも見えない。
    let retention_days = config::parse_retention_days(
        env.var("RETENTION_DAYS")
            .map(|v| v.to_string())
            .ok()
            .as_deref(),
    );

    let now_secs = Date::now().as_millis() / 1000;
    let oldest = now_secs.saturating_sub(retention_days * 24 * 60 * 60);

    // ワークスペース URL は全チャンネルで共通なので、最初に必要になった時点で
    // 1 回だけ取得して使い回す。
    let mut workspace: Option<String> = None;
    for channel in &channels {
        // 1 チャンネルの失敗（未招待・削除済みなど）で残りを止めない。
        if let Err(e) = run_channel(
            &token,
            channel,
            &done_reaction,
            oldest,
            now_secs,
            retention_days,
            &mut workspace,
        )
        .await
        {
            console_error!("nostos: channel {channel}: {e}");
        }
    }
    Ok(())
}

async fn run_channel(
    token: &str,
    channel: &str,
    done_reaction: &str,
    oldest: u64,
    now_secs: u64,
    retention_days: u64,
    workspace: &mut Option<String>,
) -> Result<()> {
    let messages = slack::fetch_history(token, channel, oldest).await?;
    let tasks = filter::unfinished_tasks(&messages, done_reaction);
    if tasks.is_empty() {
        return Ok(());
    }

    if workspace.is_none() {
        *workspace = Some(slack::workspace_url(token).await?);
    }
    let Some(workspace_url) = workspace.as_deref() else {
        return Err(Error::RustError("workspace URL is not set".to_string()));
    };

    let parent_ts =
        slack::post_message(token, channel, &filter::format_header(tasks.len()), None).await?;

    let replies = filter::format_thread_replies(&tasks, now_secs, retention_days, |m| {
        slack::permalink(workspace_url, channel, &m.ts)
    });
    for reply in replies {
        slack::post_message(token, channel, &reply, Some(&parent_ts)).await?;
    }
    Ok(())
}

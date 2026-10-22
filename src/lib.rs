use worker::*;

pub mod filter;
pub mod slack;

// Slack 無料版の閲覧制限に合わせる。
const LOOKBACK_DAYS: u64 = 90;
const DEFAULT_DONE_REACTION: &str = "done";

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Err(e) = run(&env).await {
        console_error!("nostos: {e}");
    }
}

async fn run(env: &Env) -> Result<()> {
    let token = env.secret("SLACK_BOT_TOKEN")?.to_string();
    let channel = env.secret("SLACK_CHANNEL_ID")?.to_string();

    // リアクション名は秘密情報ではないので、未設定なら既定値を使う。
    let done_reaction = env
        .var("DONE_REACTION")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| DEFAULT_DONE_REACTION.to_string());

    let now_secs = Date::now().as_millis() / 1000;
    let oldest = now_secs.saturating_sub(LOOKBACK_DAYS * 24 * 60 * 60);

    let messages = slack::fetch_history(&token, &channel, oldest).await?;
    let tasks = filter::unfinished_tasks(&messages, &done_reaction);
    if tasks.is_empty() {
        return Ok(());
    }

    let workspace = slack::workspace_url(&token).await?;
    let parent_ts =
        slack::post_message(&token, &channel, &filter::format_header(tasks.len()), None).await?;

    let replies =
        filter::format_thread_replies(&tasks, |m| slack::permalink(&workspace, &channel, &m.ts));
    for reply in replies {
        slack::post_message(&token, &channel, &reply, Some(&parent_ts)).await?;
    }
    Ok(())
}

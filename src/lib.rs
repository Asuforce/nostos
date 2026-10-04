use worker::*;

pub mod slack;

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, _env: Env, _ctx: ScheduleContext) {
    console_log!("nostos: scheduled event fired");
}

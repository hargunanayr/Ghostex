//! Delivers a Ghost's habit reports into its chat and puts the finished run's session to sleep.
//! What a report says, and which runs stay quiet, is `crate::ghosts::habits`.

use super::*;

use crate::ghosts::{habit_report_message, set_habit_report_sender, GhostHabitReport};

pub(crate) fn start_ghost_habit_runtime(state: Arc<AppState>) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<GhostHabitReport>();
    set_habit_report_sender(sender);
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let report = tokio::select! {
                _ = shutdown.recv() => break,
                report = receiver.recv() => match report {
                    Some(report) => report,
                    None => break,
                },
            };
            deliver(&state, report).await;
        }
    });
}

async fn deliver(state: &Arc<AppState>, report: GhostHabitReport) {
    if let Some(message) = habit_report_message(&report) {
        // The same delivery a thread report takes: typed now, picked up by a busy Ghost at its
        // next input boundary, and a sleeping Ghost is woken for it.
        let body = json!({
            "params": {
                "projectId": report.home_project_id,
                "sessionId": report.chat_session_id,
                "text": message,
                "sendRequestId": format!("ghost-habit-{}", report.run_session_id),
            }
        });
        let routed = crate::session_chat_send::handle_send_session_chat_message_http(
            state,
            "/api/sendSessionChatMessage".to_string(),
            Uuid::new_v4().to_string(),
            &body,
        )
        .await;
        if !routed.response.status().is_success() {
            let _ = state.logger.log(crate::logging::GxserverLogInput {
                level: crate::logging::LogLevel::Error,
                event: "ghostHabitReportFailed".into(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(format!("HTTP {}", routed.response.status())),
                details: Some(json!({
                    "projectId": report.home_project_id,
                    "sessionId": report.chat_session_id,
                    "habit": report.habit_name,
                })),
            });
        }
    }
    // The run is over and nobody opens its session, so its agent does not keep running.
    let sleeper = state.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let mut params = Map::new();
        params.insert("projectId".into(), Value::String(report.home_project_id));
        params.insert("sessionId".into(), Value::String(report.run_session_id));
        params.insert("reason".into(), json!("ghostHabitDone"));
        params.insert("sleepTrigger".into(), json!("automatic"));
        dispatch_zmx_lifecycle_http_blocking(
            &sleeper,
            "/api/sleepSession".to_string(),
            "ghost-habit-runtime".to_string(),
            params,
        )
    })
    .await;
}

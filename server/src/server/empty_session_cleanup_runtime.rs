//! Runs the new-session cleanup after a user's new-session create, and the Auto Close Empty
//! Sessions sweep: reads each candidate's agent input box and closes the ones that are still fully
//! empty through `/api/transitionSession`.
//!
//! SEE-ALSO: server/src/empty_session_cleanup.rs (the marker, the rule, CDXC:Sessions 2026-10-09 and 2026-10-28).

use super::*;

use crate::empty_session_cleanup::{
    auto_close_minutes, empty_session_candidates, idle_empty_session_candidates,
    EmptySessionCandidate,
};

/// The `/api/transitionSession` reason of every close the new-session cleanup makes.
const CLOSE_REASON: &str = "replacedByNewSession";
/// The `/api/transitionSession` reason of every close Auto Close Empty Sessions makes.
const AUTO_CLOSE_REASON: &str = "emptySessionIdle";
/// How often the Auto Close sweep runs.
const AUTO_CLOSE_INTERVAL: Duration = Duration::from_secs(60);
/// No sweep before clients have had one renewal (60 s) to report what they show again, as Auto
/// Sleep waits (CDXC:SessionSleep 2026-09-25 in session_auto_sleep_sweep.rs).
const AUTO_CLOSE_STARTUP_GRACE: Duration = Duration::from_secs(90);

pub(crate) fn start_empty_session_auto_close_sweep(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    let started_ms = chrono::Utc::now().timestamp_millis();
    tokio::spawn(async move {
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(AUTO_CLOSE_STARTUP_GRACE) => {}
        }
        let mut tick = tokio::time::interval(AUTO_CLOSE_INTERVAL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tick.tick() => {}
            }
            let now_ms = chrono::Utc::now().timestamp_millis();
            if auto_close_minutes(&state.paths) == 0
                || !crate::session_auto_sleep::sweep_allowed(
                    state.event_hub.subscriber_count(),
                    now_ms,
                )
            {
                continue;
            }
            for candidate in read_idle_candidates(&state, started_ms).await {
                if !input_box_is_empty(&candidate).await {
                    continue;
                }
                // Read again right before closing: a client may have opened it meanwhile.
                let still_idle =
                    read_idle_candidates(&state, started_ms)
                        .await
                        .iter()
                        .any(|again| {
                            again.project_id == candidate.project_id
                                && again.session_id == candidate.session_id
                        });
                if still_idle {
                    let state = state.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        close_session(
                            &state,
                            &candidate.project_id,
                            &candidate.session_id,
                            AUTO_CLOSE_REASON,
                            json!({ "trigger": "idle" }),
                        )
                    })
                    .await;
                }
            }
        }
    });
}

async fn read_idle_candidates(
    state: &Arc<AppState>,
    started_ms: i64,
) -> Vec<EmptySessionCandidate> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let minutes = auto_close_minutes(&state.paths);
        if minutes == 0 {
            return None;
        }
        let settings = crate::session_lifecycle::read_sidebar_settings(&state.paths)?;
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        idle_empty_session_candidates(
            &db,
            &repository,
            &settings,
            minutes,
            started_ms,
            chrono::Utc::now().timestamp_millis(),
        )
        .ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// CDXC:Sessions 2026-10-04 WHY:
/// A chat composer pushes its draft when it loses focus, which is the moment the new session takes it, so the create can land before that push. Waiting this long lets the push arrive before the old session is judged empty.
const SETTLE: Duration = Duration::from_secs(2);

pub(crate) fn schedule_empty_session_cleanup(
    state: Arc<AppState>,
    project_id: String,
    new_session_id: String,
) {
    tokio::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        for candidate in read_candidates(&state, &project_id, &new_session_id).await {
            if !input_box_is_empty(&candidate).await {
                continue;
            }
            // Read again right before closing: the user may have typed into its chat meanwhile.
            let still_empty = read_candidates(&state, &project_id, &new_session_id)
                .await
                .iter()
                .any(|again| again.session_id == candidate.session_id);
            if still_empty {
                let (state, project_id) = (state.clone(), project_id.clone());
                let new_session_id = new_session_id.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    close_session(
                        &state,
                        &project_id,
                        &candidate.session_id,
                        CLOSE_REASON,
                        json!({ "trigger": "newSession", "newSessionId": new_session_id }),
                    )
                })
                .await;
            }
        }
    });
}

async fn read_candidates(
    state: &Arc<AppState>,
    project_id: &str,
    new_session_id: &str,
) -> Vec<EmptySessionCandidate> {
    let state = state.clone();
    let (project_id, new_session_id) = (project_id.to_string(), new_session_id.to_string());
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        empty_session_candidates(&db, &repository, &project_id, &new_session_id).ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// Only an input box the agent's screen grammar recognises as empty counts; a screen that cannot
/// be read, or an agent mid-dialog, holds text as far as this rule knows.
async fn input_box_is_empty(candidate: &EmptySessionCandidate) -> bool {
    crate::session_chat_send::capture_session_terminal_text_vt(&candidate.zmx_name)
        .await
        .and_then(|screen| {
            crate::session_chat_composer::session_chat_composer_input(&candidate.agent_id, &screen)
        })
        .is_some_and(|input| input.is_empty())
}

/// Closes one empty session through `/api/transitionSession` and logs it.
///
/// CDXC:Sessions 2026-10-15 WHY:
/// Issue #236 found empty-session closes impossible to diagnose: the session just vanished and no persisted log said why. Closing a session the user did not close is rare (the setting is off by default) and removes what they see, so each close is an important entry, persisted without a diagnostic area, naming the session, the reason and what triggered it.
fn close_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    reason: &str,
    mut details: Value,
) -> bool {
    let mut params = Map::new();
    params.insert("action".into(), json!("close"));
    params.insert("projectId".into(), json!(project_id));
    params.insert("reason".into(), json!(reason));
    params.insert("sessionId".into(), json!(session_id));
    let response = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/transitionSession".to_string(),
        "empty-session-cleanup".to_string(),
        params,
    );
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    }
    let closed = response.response.status().is_success();
    if closed {
        if let Some(details) = details.as_object_mut() {
            details.insert("projectId".into(), json!(project_id));
            details.insert("reason".into(), json!(reason));
            details.insert("sessionId".into(), json!(session_id));
        }
        let _ = state.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "emptySessionClosed".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        });
    }
    closed
}

//! The Board's routes: complete or reopen a thread, complete many, and start, edit or read a Board
//! task. The rules live in `crate::session_board`.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_project_id, read_session_id, DomainStateError};
use crate::protocol::rpc_success;
use crate::session_board::{
    begin_session_task, read_session_task, rollback_session_task_start, set_session_completed,
    update_session_task, CompletionFollowUp,
};

use super::*;

/// The pause between the sleeps of a Complete all.
///
/// CDXC:SessionBoard 2026-10-28 SEE-ALSO: the same pacing as `BULK_SLEEP_INTERVAL_MS` in packages/gx-core/src/sidebar_actions/bulk.rs, so a long Review backlog does not stop every daemon at once.
const COMPLETE_ALL_SLEEP_INTERVAL_MS: u64 = 350;

pub(super) async fn route_session_board_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/completeSession" => respond(
            endpoint.path,
            request_id,
            complete_session(&state, &body_json).await,
        ),
        "/api/completeSessions" => respond(
            endpoint.path,
            request_id,
            complete_sessions(&state, &body_json),
        ),
        "/api/startSessionTask" => respond(
            endpoint.path,
            request_id,
            start_session_task(&state, &body_json),
        ),
        "/api/updateSessionTask" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_project_id(params)?;
                let session_id = read_session_id(params)?;
                let session = update_session_task(
                    repository,
                    &project_id,
                    &session_id,
                    params.get("title").and_then(Value::as_str),
                    params.get("prompt").and_then(Value::as_str),
                )?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/readSessionTask" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                read_session_task(
                    repository,
                    &read_project_id(params)?,
                    &read_session_id(params)?,
                )
            },
        ),
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}

fn respond(
    endpoint_path: String,
    request_id: String,
    result: Result<Value, DomainStateError>,
) -> RoutedResponse {
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn open_repository_db(
    state: &AppState,
) -> Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })
}

fn session_params(project_id: &str, session_id: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params
}

/// Marks the finished turn seen and sleeps the session, through the same RPCs the sidebar's own
/// acknowledge and Sleep send, so each one publishes its own change.
fn run_completion_follow_up(state: &AppState, follow_up: &CompletionFollowUp) {
    if follow_up.acknowledge {
        let mut params = session_params(&follow_up.project_id, &follow_up.session_id);
        params.insert("event".to_string(), json!("acknowledge"));
        let _ = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/updateAgentActivity".to_string(),
            format!("board-complete-{}", uuid::Uuid::new_v4()),
            params,
        );
    }
    if follow_up.sleep {
        let _ = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/sleepSession".to_string(),
            format!("board-complete-{}", uuid::Uuid::new_v4()),
            session_params(&follow_up.project_id, &follow_up.session_id),
        );
    }
}

/// `{ projectId, sessionId, completed?: bool }`: Complete (the default) or Reopen.
async fn complete_session(state: &Arc<AppState>, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let project_id = read_project_id(&params)?;
    let session_id = read_session_id(&params)?;
    let completed = params
        .get("completed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let outcome = {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let outcome =
            set_session_completed(&repository, &project_id, &session_id, completed, &now_iso())?;
        if outcome.changed {
            schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
        }
        outcome
    };
    if let Some(follow_up) = outcome.follow_up {
        let worker = state.clone();
        let _ = tokio::task::spawn_blocking(move || run_completion_follow_up(&worker, &follow_up))
            .await;
    }
    Ok(json!({
        "changed": outcome.changed,
        "completed": completed,
        "session": outcome.session,
    }))
}

/// `{ sessions: [{ projectId, sessionId }] }`: Complete all. The flags are written now; the sleeps
/// run after the answer, one at a time. A thread that cannot be completed (working, never started)
/// is reported in `skipped` and the others still complete.
fn complete_sessions(state: &Arc<AppState>, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let targets = params
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| DomainStateError::bad_request("Pass sessions: [{ projectId, sessionId }]."))?;
    let now = now_iso();
    let mut completed = Vec::new();
    let mut skipped = Vec::new();
    let mut follow_ups = Vec::new();
    {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        for target in targets {
            let Some(target) = target.as_object() else {
                continue;
            };
            let (project_id, session_id) = (read_project_id(target)?, read_session_id(target)?);
            match set_session_completed(&repository, &project_id, &session_id, true, &now) {
                Ok(outcome) => {
                    if outcome.changed {
                        schedule_presentation_session_delta(
                            state,
                            &db,
                            &repository,
                            &project_id,
                            &session_id,
                        )?;
                    }
                    follow_ups.extend(outcome.follow_up);
                    completed.push(json!({ "projectId": project_id, "sessionId": session_id }));
                }
                Err(error) => skipped.push(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "reason": error.message,
                })),
            }
        }
    }
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        for (index, follow_up) in follow_ups.iter().enumerate() {
            if index > 0 && follow_up.sleep {
                std::thread::sleep(std::time::Duration::from_millis(
                    COMPLETE_ALL_SLEEP_INTERVAL_MS,
                ));
            }
            run_completion_follow_up(&worker, follow_up);
        }
    });
    Ok(json!({ "completed": completed, "skipped": skipped }))
}

/// `{ projectId, sessionId }`: Start on a Board task. Queues the task's prompt as the draft's
/// startup send and wakes its agent; the queue types it once the agent's input box is up.
fn start_session_task(state: &Arc<AppState>, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let project_id = read_project_id(&params)?;
    let session_id = read_session_id(&params)?;
    let prompt = {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        match begin_session_task(&repository, &project_id, &session_id, &now_iso())? {
            Err(refusal) => return Ok(json!({ "started": false, "reason": refusal.as_str() })),
            Ok(prompt) => {
                schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                )?;
                prompt
            }
        }
    };
    if let Err(error) =
        crate::server::queue_session_chat_prompt(state, &project_id, &session_id, &prompt, true)
    {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        rollback_session_task_start(&repository, &project_id, &session_id)?;
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
        return Err(error);
    }
    let target = crate::session_chat_send::resolve_session_chat_send_target(
        state,
        &params,
        "startSessionTask",
    )?;
    crate::session_chat_send_wake::start_draft_agent_if_missing(state, &target);
    Ok(json!({ "started": true }))
}

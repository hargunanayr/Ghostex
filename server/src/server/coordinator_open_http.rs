//! `/api/openCoordinatorThread`: a row of a coordinator's Threads panel was clicked.
//!
//! CDXC:Coordinators 2026-10-08 WHY:
//! The user: "when I click on this session in the threads list ... it's not opening it". Most of a coordinator's threads are resolved and their sessions closed (`stopped`), and a closed session has no live terminal or chat to open, so the click must resume it first. Each client used to decide that on its own: the desktop and the phone woke a `stopped` row and the web build never did, so a click there showed a dead transcript. gxserver now decides once for every client: it checks the thread belongs to the coordinator whose chat asks, resumes its session when it is closed (the same `/api/wakeSession` `ghostex coordinator reopen` uses), and only then does the chat core ask its host to focus it. Opening a thread does not reopen its task: the thread stays resolved, so the coordinator is not sent its reports again (`ghostex coordinator reopen` does that on purpose).
//! SEE-ALSO: packages/gx-chat-core/src/extras/coordinator_threads.rs (`open_thread`, the call and the focus that follows), apps/desktop/src/app/session_chat_fork_branches.rs, apps/gpui-web/src/app/chat_host.rs and apps/mobile/app/src/chat/native/NativeChatScreen.tsx (the focus).

use super::*;

use crate::coordinators::read_thread;

pub(crate) async fn handle_open_coordinator_thread_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let worker_state = state.clone();
    let worker_endpoint = endpoint_path.clone();
    let worker_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        let wake_request_id = worker_request_id.clone();
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body,
            |repository, db, params, _| {
                open_thread(&worker_state, repository, db, params, wake_request_id)
            },
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::corrupt_state(format!("openCoordinatorThread failed: {error}")),
        ),
    }
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

/// `projectId`/`sessionId` name the coordinator (the chat that asks), `threadProjectId`/
/// `threadSessionId` the thread. Answers `{projectId, sessionId, resumed}` for the thread.
fn open_thread(
    state: &AppState,
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
    request_id: String,
) -> std::result::Result<Value, DomainStateError> {
    let coordinator = (
        param(params, "projectId")?.to_string(),
        param(params, "sessionId")?.to_string(),
    );
    let project_id = param(params, "threadProjectId")?;
    let session_id = param(params, "threadSessionId")?;
    let thread = read_thread(db, project_id, session_id)?
        .filter(|thread| thread.coordinator_key() == coordinator)
        .ok_or_else(|| {
            DomainStateError::not_found("That session is not a thread of this coordinator.")
        })?;
    let session = repository
        .get_session(&thread.project_id, &thread.session_id)?
        .ok_or_else(|| DomainStateError::not_found("That thread's session no longer exists."))?;
    let closed = session.get("lifecycleState").and_then(Value::as_str) == Some("stopped");
    if closed {
        let mut wake = Map::new();
        wake.insert("projectId".into(), json!(thread.project_id));
        wake.insert("sessionId".into(), json!(thread.session_id));
        let woken = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/wakeSession".to_string(),
            request_id,
            wake,
        );
        if !woken.response.status().is_success() {
            return Err(DomainStateError::corrupt_state(
                "The thread's session could not be resumed. Try `ghostex coordinator reopen` for it.",
            ));
        }
    }
    Ok(json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "resumed": closed,
    }))
}

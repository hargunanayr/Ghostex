//! `/api/acknowledgeCoordinatorThreads`: marks an orchestrator's finished threads seen.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use super::records::{list_threads_for, SessionKey};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// A thread's own finished turn the user has not seen yet: the blue dot, never a needs-you state.
fn is_finished_unseen(session: &Value) -> bool {
    crate::session_status::is_turn_complete_attention(
        session.pointer("/runtimeSettings/agentActivity"),
    ) && crate::session_chat_async_questions::pending_question_count(session) == 0
        && crate::session_chat_open_questions::open_question_count(session) == 0
}

/// Every thread under the orchestrator (its threads' threads too) whose finished turn is unseen.
fn finished_unseen_threads(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    coordinator: &SessionKey,
) -> Result<Vec<SessionKey>, DomainStateError> {
    let mut seen: BTreeSet<SessionKey> = BTreeSet::from([coordinator.clone()]);
    let mut pending = vec![coordinator.clone()];
    let mut finished = Vec::new();
    while let Some((project_id, session_id)) = pending.pop() {
        for thread in list_threads_for(db, &project_id, &session_id)? {
            let key = thread.key();
            if !seen.insert(key.clone()) {
                continue;
            }
            if repository
                .get_session(&key.0, &key.1)?
                .is_some_and(|session| is_finished_unseen(&session))
            {
                finished.push(key.clone());
            }
            pending.push(key);
        }
    }
    Ok(finished)
}

/// Acknowledges the orchestrator's finished-unseen threads the way a sidebar click on each would,
/// so their blue dot clears on every client. Runs on the blocking pool.
///
/// Every thread under it, at any depth, whose finished turn is unseen is acknowledged; a thread that waits on the user (a question, an approval, or any attention that is not a finished turn) keeps its dot. gxserver decides which threads qualify because only it knows why a session is in attention.
///
/// CDXC:Coordinators 2026-10-28 WHY:
/// The desktop and the web build no longer call this when the user views an orchestrator (the user's 2026-10-28 decision on gx-core `attention::acknowledge`: a finished thread counts as seen only when the thread itself is opened). It is still reached through `ghostex orchestrator acknowledge-threads`, which the phone's orchestrator row tap runs (apps/mobile/app/src/terminal/attention.ts) until the phone follows that decision.
pub(crate) fn acknowledge_coordinator_threads(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return Err(DomainStateError::bad_request(
            "This orchestrator request needs projectId and sessionId.".to_string(),
        ));
    };
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let threads = finished_unseen_threads(&db, &repository, &(project_id, session_id))?;
    drop(db);
    for (project_id, session_id) in &threads {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("event".to_string(), json!("acknowledge"));
        let _ = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/updateAgentActivity".to_string(),
            format!("coordinator-seen-{}", uuid::Uuid::new_v4()),
            params,
        );
    }
    Ok(json!({
        "ok": true,
        "acknowledged": threads
            .iter()
            .map(|(project_id, session_id)| json!({ "projectId": project_id, "sessionId": session_id }))
            .collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinators::link_thread;
    use crate::paths::get_gxserver_paths;
    use crate::storage::initialize_gxserver_storage;

    fn params(entries: Value) -> Map<String, Value> {
        entries.as_object().cloned().expect("params object")
    }

    fn session(
        repository: &DomainRepository<'_>,
        project_id: &str,
        activity: Value,
        extra: Value,
    ) -> SessionKey {
        let mut runtime = params(json!({ "agentActivity": activity }));
        runtime.extend(params(extra));
        let session = repository
            .create_session(
                &params(json!({
                    "projectId": project_id,
                    "kind": "agent",
                    "agentId": "claude",
                    "title": "Thread",
                    "lifecycleState": "running",
                    "runtimeSettings": Value::Object(runtime),
                })),
                false,
            )
            .expect("create session");
        let text = |key: &str| session[key].as_str().expect(key).to_string();
        (text("projectId"), text("sessionId"))
    }

    /// Viewing the orchestrator clears its threads' finished turns, a nested thread's too, and
    /// leaves every needs-you state (an approval prompt, a question card) and the working one alone.
    #[test]
    fn viewing_an_orchestrator_picks_only_its_finished_unseen_threads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, "S1");
        let project_id = repository
            .create_project(&params(
                json!({ "name": "Seen", "path": std::env::temp_dir() }),
            ))
            .expect("create project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let finished = json!({ "activity": "attention", "attentionSource": "turnComplete" });
        let lead = session(
            &repository,
            &project_id,
            json!({ "activity": "idle" }),
            json!({}),
        );
        let done = session(&repository, &project_id, finished.clone(), json!({}));
        let approval = session(
            &repository,
            &project_id,
            json!({ "activity": "attention", "attentionSource": "notification" }),
            json!({}),
        );
        let asking = session(
            &repository,
            &project_id,
            finished.clone(),
            json!({ "sessionChatAsyncQuestionIds": ["q1"] }),
        );
        let working = session(
            &repository,
            &project_id,
            json!({ "activity": "working" }),
            json!({}),
        );
        let nested = session(&repository, &project_id, finished.clone(), json!({}));
        let seen = session(
            &repository,
            &project_id,
            json!({ "activity": "attention", "attentionSource": "turnComplete", "isAcknowledged": true }),
            json!({}),
        );
        let stranger = session(&repository, &project_id, finished, json!({}));
        for thread in [&done, &approval, &asking, &working, &seen] {
            link_thread(&db, &thread.0, &thread.1, &lead.0, &lead.1, "task").expect("link");
        }
        link_thread(&db, &nested.0, &nested.1, &done.0, &done.1, "task").expect("link nested");
        let _ = stranger;

        let mut picked = finished_unseen_threads(&db, &repository, &lead).expect("threads");
        picked.sort();
        let mut expected = vec![done, nested];
        expected.sort();
        assert_eq!(picked, expected);
    }
}

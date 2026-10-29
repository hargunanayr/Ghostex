//! Board tasks: a thread made with the Board's Add task (or `ghostex create-agent --task`) that
//! waits in Pending with its prompt until the user starts it.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): a ticket made from the Board waits in Pending with its prompt, and its agent is not started until he clicks Start ("Nunggu di Pending sampai Start"). A task is a draft created without a provider (`draft: true`, no `startSessionProvider`) whose prompt is kept in `runtimeSettings.pendingTaskPrompt`. Start queues that prompt as the draft's startup send and wakes its agent, the path a first chat message to a draft takes (session_chat_send/send_http.rs). The prompt leaves with the draft marker (`clear_draft_status` clears both), so a task the user prompts some other way becomes an ordinary thread.

use serde_json::{json, Map, Value};

use crate::agents::{object_field, session_is_draft};
use crate::domain::{DomainRepository, DomainStateError};

pub(crate) const PENDING_TASK_PROMPT_KEY: &str = "pendingTaskPrompt";
pub(crate) const PENDING_TASK_STARTED_AT_KEY: &str = "pendingTaskStartedAt";
/// How much of the prompt every snapshot carries for the card; the edit panel reads the rest.
const TASK_PREVIEW_CHARS: usize = 400;

/// Arms a task at creation: only a create that is a draft and names a prompt makes one. A draft
/// rebuilt for another agent (agents/drafts.rs) carries its runtime settings, prompt included.
pub(crate) fn apply_pending_task_create_param(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) {
    if params.get("draft").and_then(Value::as_bool) != Some(true) {
        clear_pending_task(runtime_settings);
        return;
    }
    if let Some(prompt) = params
        .get(PENDING_TASK_PROMPT_KEY)
        .and_then(Value::as_str)
        .filter(|prompt| !prompt.trim().is_empty())
    {
        runtime_settings.insert(PENDING_TASK_PROMPT_KEY.to_string(), json!(prompt));
        runtime_settings.remove(PENDING_TASK_STARTED_AT_KEY);
    }
}

pub(crate) fn clear_pending_task(runtime_settings: &mut Map<String, Value>) {
    runtime_settings.remove(PENDING_TASK_PROMPT_KEY);
    runtime_settings.remove(PENDING_TASK_STARTED_AT_KEY);
}

fn runtime_text<'a>(session: &'a Value, key: &str) -> Option<&'a str> {
    session
        .get("runtimeSettings")?
        .get(key)?
        .as_str()
        .filter(|value| !value.trim().is_empty())
}

/// The task's prompt while the session is still a draft.
pub(crate) fn pending_task_prompt(session: &Value) -> Option<&str> {
    session_is_draft(session)
        .then(|| runtime_text(session, PENDING_TASK_PROMPT_KEY))
        .flatten()
}

/// The card's two published fields: the prompt cut for the card, and when Start was pressed.
pub(crate) fn pending_task_presentation(session: &Value) -> Option<(String, Option<String>)> {
    let prompt = pending_task_prompt(session)?;
    let collapsed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = if collapsed.chars().count() > TASK_PREVIEW_CHARS {
        let mut cut = collapsed.chars().take(TASK_PREVIEW_CHARS).collect::<String>();
        cut.push('…');
        cut
    } else {
        collapsed
    };
    Some((
        preview,
        runtime_text(session, PENDING_TASK_STARTED_AT_KEY).map(str::to_string),
    ))
}

pub(crate) fn validate_task_prompt(prompt: &str) -> Result<String, DomainStateError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(DomainStateError::bad_request("A task needs a prompt."));
    }
    if prompt.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "The task prompt exceeds the {}-byte send limit.",
            crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
        )));
    }
    Ok(prompt.to_string())
}

fn require_waiting_task(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })?;
    if pending_task_prompt(&session).is_none() {
        return Err(DomainStateError::bad_request(format!(
            "Session {project_id}/{session_id} is not a Board task waiting to start."
        )));
    }
    Ok(session)
}

fn runtime_update(
    project_id: &str,
    session_id: &str,
    runtime_settings: Map<String, Value>,
) -> Map<String, Value> {
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update
}

/// Why a Start did not begin, for the `{ started: false, reason }` answer.
pub(crate) enum TaskStartRefusal {
    AlreadyStarted,
}

impl TaskStartRefusal {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            TaskStartRefusal::AlreadyStarted => "alreadyStarted",
        }
    }
}

/// Stamps `pendingTaskStartedAt` (the card's "Starting…") and returns the prompt to send.
pub(crate) fn begin_session_task(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    now_iso: &str,
) -> Result<Result<String, TaskStartRefusal>, DomainStateError> {
    let session = require_waiting_task(repository, project_id, session_id)?;
    if runtime_text(&session, PENDING_TASK_STARTED_AT_KEY).is_some() {
        return Ok(Err(TaskStartRefusal::AlreadyStarted));
    }
    let prompt = validate_task_prompt(pending_task_prompt(&session).unwrap_or_default())?;
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(PENDING_TASK_STARTED_AT_KEY.to_string(), json!(now_iso));
    repository.update_session(&runtime_update(project_id, session_id, runtime_settings))?;
    Ok(Ok(prompt))
}

/// Undoes `begin_session_task` when the prompt could not be queued, so the card offers Start again.
pub(crate) fn rollback_session_task_start(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(());
    };
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    if runtime_settings.remove(PENDING_TASK_STARTED_AT_KEY).is_some() {
        repository.update_session(&runtime_update(project_id, session_id, runtime_settings))?;
    }
    Ok(())
}

/// Edits a waiting task's title and prompt. A title set here is the user's, so title generation
/// leaves it alone.
pub(crate) fn update_session_task(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    title: Option<&str>,
    prompt: Option<&str>,
) -> Result<Value, DomainStateError> {
    let session = require_waiting_task(repository, project_id, session_id)?;
    if runtime_text(&session, PENDING_TASK_STARTED_AT_KEY).is_some() {
        return Err(DomainStateError::bad_request(
            "This task has already been started.",
        ));
    }
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    if let Some(prompt) = prompt {
        runtime_settings.insert(
            PENDING_TASK_PROMPT_KEY.to_string(),
            json!(validate_task_prompt(prompt)?),
        );
    }
    let mut update = runtime_update(project_id, session_id, runtime_settings);
    if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
        update.insert("title".to_string(), json!(title));
        if let Some(Value::Object(settings)) = update.get_mut("runtimeSettings") {
            settings.insert("titleSource".to_string(), json!("user"));
        }
    }
    repository.update_session(&update)
}

/// The full task for the edit panel.
pub(crate) fn read_session_task(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let session = require_waiting_task(repository, project_id, session_id)?;
    Ok(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "title": session.get("title").cloned().unwrap_or(Value::Null),
        "prompt": pending_task_prompt(&session),
        "startedAt": runtime_text(&session, PENDING_TASK_STARTED_AT_KEY),
    }))
}

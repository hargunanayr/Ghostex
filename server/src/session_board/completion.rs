//! The Board's Completed column: the `completedAt` flag the user sets on a thread, and the rule
//! that real work clears it again.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): a thread that passed Review "bisa di mark as completed. Threads yg di mark as completed, langsung saja di buat sleep. Dan sediakan juga tombol close all untuk yang sudah completed." Completing a thread stores `completedAt` on its row, marks a finished-but-unseen turn as seen, and puts the session to sleep as the user's own Sleep (no auto-sleep guard). Reopen clears the flag and leaves the session asleep. Close all is the ordinary bulk close of the Completed cards (gx-core session_board/actions.rs). Complete all skips threads waiting on an answer; the client leaves them out of the list it sends.

use serde_json::{json, Map, Value};

use crate::agents::session_is_draft;
use crate::domain::{DomainRepository, DomainStateError};

pub(crate) const COMPLETED_AT_KEY: &str = "completedAt";

fn stored_activity(runtime_settings: Option<&Value>) -> Option<&str> {
    runtime_settings?
        .get("agentActivity")?
        .get("activity")?
        .as_str()
}

/// The agent started working: the merged runtime settings say working and the stored ones did not.
///
/// CDXC:SessionBoard 2026-10-28 WHY: The flag is cleared in `merge_session_update`, the one merge every activity write passes through (agents/activity.rs and agents/session_state_ingest.rs), the way setting isPinned clears isParked, so no writer can forget it and a thread the user prompts after completing it goes back through In progress and Review. Opening or waking a completed thread resets activity to idle, never working, so it stays completed.
pub(crate) fn clears_completion(
    previous_runtime_settings: Option<&Value>,
    next_runtime_settings: &Map<String, Value>,
) -> bool {
    let next = next_runtime_settings
        .get("agentActivity")
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str);
    next == Some("working") && stored_activity(previous_runtime_settings) != Some("working")
}

pub(crate) fn session_is_completed(session: &Value) -> bool {
    session
        .get(COMPLETED_AT_KEY)
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

/// A finished turn the user has not seen yet (the blue dot). A question or a permission prompt has
/// another attention source and keeps waiting.
fn has_unseen_turn(session: &Value) -> bool {
    let activity = session.pointer("/runtimeSettings/agentActivity");
    activity
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str)
        == Some("attention")
        && activity
            .and_then(|activity| activity.get("attentionSource"))
            .and_then(Value::as_str)
            == Some(crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE)
}

/// What completing a thread still has to do after its flag is written: both go through the same
/// RPCs a client would call, so they emit their own presentation deltas.
pub(crate) struct CompletionFollowUp {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) acknowledge: bool,
    pub(crate) sleep: bool,
}

pub(crate) struct CompletionOutcome {
    pub(crate) session: Value,
    pub(crate) changed: bool,
    pub(crate) follow_up: Option<CompletionFollowUp>,
}

fn completion_update(project_id: &str, session_id: &str, completed_at: Value) -> Map<String, Value> {
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(COMPLETED_AT_KEY.to_string(), completed_at);
    update
}

/// A thread that is working has nothing to review yet, and a draft has never been started.
fn refuse_completion(session: &Value, session_ref: &str, now_iso: &str) -> Result<(), DomainStateError> {
    if crate::session_lifecycle::session_activity(session, now_iso) == "working" {
        return Err(DomainStateError::bad_request(format!(
            "Session {session_ref} is working and cannot be completed."
        )));
    }
    if session_is_draft(session) {
        return Err(DomainStateError::bad_request(format!(
            "Session {session_ref} has not started yet and cannot be completed."
        )));
    }
    Ok(())
}

/// Sets (`completed`) or clears (Reopen) a thread's `completedAt`. Completing an already completed
/// thread changes nothing but still sleeps it if it was woken since.
pub(crate) fn set_session_completed(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    completed: bool,
    now_iso: &str,
) -> Result<CompletionOutcome, DomainStateError> {
    let session_ref = format!("{project_id}/{session_id}");
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("Session {session_ref} does not exist.")))?;
    let already = session_is_completed(&session);
    if !completed {
        if !already {
            return Ok(CompletionOutcome {
                session,
                changed: false,
                follow_up: None,
            });
        }
        let session =
            repository.update_session(&completion_update(project_id, session_id, Value::Null))?;
        return Ok(CompletionOutcome {
            session,
            changed: true,
            follow_up: None,
        });
    }
    refuse_completion(&session, &session_ref, now_iso)?;
    let (session, changed) = if already {
        (session, false)
    } else {
        (
            repository.update_session(&completion_update(project_id, session_id, json!(now_iso)))?,
            true,
        )
    };
    let follow_up = CompletionFollowUp {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        acknowledge: has_unseen_turn(&session),
        sleep: crate::presentation::effective_lifecycle_state(&session) == "running",
    };
    Ok(CompletionOutcome {
        session,
        changed,
        follow_up: Some(follow_up),
    })
}

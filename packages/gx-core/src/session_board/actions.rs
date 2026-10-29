//! What the Board's buttons and drops send. The routes are gxserver's
//! (`server/src/server/route_http/session_board.rs`); a host only carries these to the card's
//! machine.

use serde_json::{json, Map, Value};

use super::columns::BoardColumn;
use super::view::{BoardCard, SessionBoardView};
use crate::keys::SessionKey;

pub const COMPLETE_SESSION_ENDPOINT: &str = "/api/completeSession";
pub const COMPLETE_SESSIONS_ENDPOINT: &str = "/api/completeSessions";
pub const START_SESSION_TASK_ENDPOINT: &str = "/api/startSessionTask";
pub const UPDATE_SESSION_TASK_ENDPOINT: &str = "/api/updateSessionTask";
pub const READ_SESSION_TASK_ENDPOINT: &str = "/api/readSessionTask";
/// The create parameter that makes a draft a Board task (server/src/session_board/pending_task.rs).
pub const PENDING_TASK_PROMPT_PARAM: &str = "pendingTaskPrompt";
/// How long a title taken from a task's prompt may be.
const TASK_TITLE_CHARS: usize = 60;

fn session_params(key: &SessionKey) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(key.project_id));
    params.insert("sessionId".to_string(), json!(key.session_id));
    params
}

/// Complete (`true`) or Reopen (`false`) one thread.
pub fn complete_params(key: &SessionKey, completed: bool) -> Value {
    let mut params = session_params(key);
    params.insert("completed".to_string(), json!(completed));
    Value::Object(params)
}

/// The Review cards Complete all completes, and how many it leaves because they wait on an answer.
pub fn complete_all_targets(view: &SessionBoardView) -> (Vec<&BoardCard>, usize) {
    let Some(lane) = view.lane(BoardColumn::Review) else {
        return (Vec::new(), 0);
    };
    let targets: Vec<&BoardCard> = lane
        .cards
        .iter()
        .filter(|card| !card.waits_on_answer())
        .collect();
    let kept = lane.cards.len() - targets.len();
    (targets, kept)
}

/// `/api/completeSessions` for Complete all, or `None` when there is nothing to complete.
pub fn complete_all_params(view: &SessionBoardView) -> Option<Value> {
    let (targets, _) = complete_all_targets(view);
    (!targets.is_empty()).then(|| {
        json!({
            "sessions": targets
                .iter()
                .map(|card| Value::Object(session_params(&card.key)))
                .collect::<Vec<_>>(),
        })
    })
}

/// Close all: the sidebar's own bulk close of the visible Completed cards, or `None` when the
/// column is empty.
pub fn close_completed_message(view: &SessionBoardView) -> Option<Value> {
    let lane = view.lane(BoardColumn::Completed)?;
    (!lane.cards.is_empty()).then(|| {
        json!({
            "type": "closeSessions",
            "sessionIds": lane
                .cards
                .iter()
                .map(|card| card.sidebar_session_id.clone())
                .collect::<Vec<_>>(),
        })
    })
}

pub fn start_task_params(key: &SessionKey) -> Value {
    Value::Object(session_params(key))
}

pub fn read_task_params(key: &SessionKey) -> Value {
    Value::Object(session_params(key))
}

pub fn update_task_params(key: &SessionKey, title: Option<&str>, prompt: &str) -> Value {
    let mut params = session_params(key);
    if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
        params.insert("title".to_string(), json!(title));
    }
    params.insert("prompt".to_string(), json!(prompt));
    Value::Object(params)
}

/// The prompt's first line, short enough for a card, for a task the user did not name.
pub fn task_title_from_prompt(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("New task");
    if line.chars().count() > TASK_TITLE_CHARS {
        let mut title: String = line.chars().take(TASK_TITLE_CHARS).collect();
        title.push('…');
        title
    } else {
        line.to_string()
    }
}

/// Turns a sidebar agent launch (`local_agent_launch_params` / `remote_agent_launch_params`, both
/// drafts) into a Board task: the prompt rides along and waits for Start. A title the user typed
/// is theirs, so title generation leaves it alone.
pub fn with_board_task(mut params: Value, prompt: &str, user_title: Option<&str>) -> Value {
    let Value::Object(object) = &mut params else {
        return params;
    };
    object.insert("draft".to_string(), json!(true));
    object.insert(PENDING_TASK_PROMPT_PARAM.to_string(), json!(prompt.trim()));
    match user_title.map(str::trim).filter(|title| !title.is_empty()) {
        Some(title) => {
            object.insert("title".to_string(), json!(title));
            if let Some(Value::Object(runtime)) = object.get_mut("runtimeSettings") {
                runtime.insert("titleSource".to_string(), json!("user"));
            }
        }
        None => {
            object.insert("title".to_string(), json!(task_title_from_prompt(prompt)));
        }
    }
    params
}

/// What dropping a card on a column does. Only Review and Completed take drops: the other
/// columns follow the thread's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardDrop {
    Complete,
    Reopen,
}

pub fn board_drop(card: &BoardCard, to: BoardColumn) -> Option<BoardDrop> {
    match (card.column, to) {
        (BoardColumn::Review, BoardColumn::Completed) => Some(BoardDrop::Complete),
        (BoardColumn::Completed, BoardColumn::Review) => Some(BoardDrop::Reopen),
        _ => None,
    }
}

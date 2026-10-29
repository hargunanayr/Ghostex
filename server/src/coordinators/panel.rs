//! The `coordinatorThreads` chat field: a coordinator's threads as its chat's Threads panel reads
//! them.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! Claude puts an Overview of the threads beside the conversation; Ghostex puts a Threads panel above the coordinator's composer, next to the Subagents and Tasks panels, so it also reaches the web build and the phone (which has no sidebar tree). The supervisor refreshes this cache every tick and republishes the coordinator's chat state when it changed; the frame builders only read the cache, because they run under the stream's emit lock and must not touch the database. Absent on a frame means unchanged (like `appCommands`), so a builder that leaves it out can never blank the panel.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (refresh and republish), server/src/session_chat_follower/frames.rs and session_chat_read.rs (carry it), packages/gx-chat-core/src/extras/coordinator_threads.rs (the panel).
//!
//! CDXC:Coordinators 2026-10-07 WHY:
//! A frame carries only a summary: the threads in flight (working, or waiting on an answer or the user's approval) and the `recent` ones (active in the last two hours, which "Show recent" adds; the user's 2026-10-11 decision in the core), the total and a `revision` of the full list. With ~95 threads the whole list was ~30KB on every chat state frame. The full list, resolved threads included, is read once when the user opens the panel's "N more" row (`/api/readCoordinatorThreads`), and read again when the revision moves while it stays open, so every thread stays reachable. The two-hour boundary is decided here on every tick, so a thread ageing out changes the summary and republishes it.
//! SEE-ALSO: packages/gx-chat-core/src/extras/coordinator_threads.rs (the panel and the read), packages/gx-core/src/sidebar_view/threads.rs (`RECENT_THREAD_MS`, the sidebar's same two hours).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::brief::report_headline;
use super::endpoint::session_title_of;
use super::records::{list_coordinators, list_threads, SessionKey, ThreadRecord};
use super::state::{
    classify_thread_session, thread_needs_user_approval, waiting_prompt, ThreadProgress,
    ThreadState,
};
use crate::domain::DomainRepository;
use crate::presentation::{effective_lifecycle_state, now_iso};

/// How long after its last activity a thread that neither works nor waits is still listed.
const RECENT_THREAD_MS: i64 = 2 * 60 * 60 * 1000;

fn is_recent(active_at: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(active_at)
        .is_ok_and(|at| now.signed_duration_since(at).num_milliseconds() < RECENT_THREAD_MS)
}

/// One coordinator's cached panel: what frames carry, and what "N more" reads.
#[derive(PartialEq)]
struct CachedPanel {
    summary: Value,
    /// Every thread, in the panel's order.
    threads: Vec<Value>,
}

fn panels() -> &'static Mutex<HashMap<SessionKey, CachedPanel>> {
    static PANELS: OnceLock<Mutex<HashMap<SessionKey, CachedPanel>>> = OnceLock::new();
    PANELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Working first, then waiting on the user (an answer or an approval), then the rest; each newest
/// first. The first two are the panel's in-flight rows, always in the summary.
fn panel_rank(row: &Value) -> u8 {
    match (row["state"].as_str(), row["needsApproval"] == true) {
        (Some("working"), _) => 0,
        (Some("waiting"), _) | (_, true) => 1,
        _ => 2,
    }
}

fn cached_panel(mut threads: Vec<Value>, previous: Option<&CachedPanel>) -> CachedPanel {
    threads.sort_by(|left, right| {
        panel_rank(left).cmp(&panel_rank(right)).then_with(|| {
            right["activeAt"]
                .as_str()
                .unwrap_or_default()
                .cmp(left["activeAt"].as_str().unwrap_or_default())
        })
    });
    // The summary is a function of the sorted list alone, so an unchanged list keeps it without
    // serialising every thread again for the revision.
    if let Some(previous) = previous.filter(|previous| previous.threads == threads) {
        return CachedPanel {
            summary: previous.summary.clone(),
            threads,
        };
    }
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&threads)
        .unwrap_or_default()
        .hash(&mut hasher);
    let summary = json!({
        "threads": threads
            .iter()
            .filter(|row| panel_rank(row) < 2 || row["recent"] == true)
            .collect::<Vec<_>>(),
        "total": threads.len(),
        "revision": format!("{:016x}", hasher.finish()),
    });
    CachedPanel { summary, threads }
}

/// `/api/readCoordinatorThreads`: every thread of the coordinator, for the panel's "N more".
pub fn read_coordinator_threads(key: &SessionKey) -> Value {
    let panels = panels().lock().ok();
    match panels.as_ref().and_then(|panels| panels.get(key)) {
        Some(panel) => json!({
            "threads": panel.threads,
            "total": panel.threads.len(),
            "revision": panel.summary["revision"],
        }),
        None => json!({ "threads": [], "total": 0, "revision": "" }),
    }
}

/// Adds `coordinatorThreads` to a chat frame or read result of a coordinator session.
pub fn insert_coordinator_threads(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let value = panels().lock().ok().and_then(|panels| {
        panels
            .get(&(project_id.to_string(), session_id.to_string()))
            .map(|panel| panel.summary.clone())
    });
    if let Some(mut value) = value {
        // CDXC:Ghosts 2026-10-29 WHY: a Ghost's chat is a coordinator underneath, but its welcome speaks as the Ghost (gx-chat-core `extras/document.rs`), so the panel names the Ghost it belongs to.
        if let Some(ghost) = crate::ghosts::published_ghost_for_project(project_id)
            .filter(|ghost| ghost["chatSessionId"].as_str() == Some(session_id))
        {
            value["ghostName"] = ghost["name"].clone();
        }
        frame.insert("coordinatorThreads".to_string(), value);
    }
}

/// One thread's panel row, kept while nothing it was built from has changed.
struct CachedRow {
    thread: ThreadRecord,
    /// A hash of the session row's stored columns, `None` when the row is gone.
    session: Option<u64>,
    row: Value,
    /// When the row's `recent` mark runs out, while it has one.
    recent_until: Option<chrono::DateTime<chrono::Utc>>,
}

fn rows() -> &'static Mutex<HashMap<SessionKey, CachedRow>> {
    static ROWS: OnceLock<Mutex<HashMap<SessionKey, CachedRow>>> = OnceLock::new();
    ROWS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The thread's session row as stored: a hash of its columns, and the row itself when `needs_row`
/// says the hash does not match what is cached.
fn read_session_row(
    statement: &mut rusqlite::CachedStatement<'_>,
    key: &SessionKey,
    needs_row: impl Fn(u64) -> bool,
) -> rusqlite::Result<Option<(u64, Option<crate::domain::SessionRow>)>> {
    statement
        .query_row([&key.0, &key.1], |row| {
            let mut hasher = DefaultHasher::new();
            for index in 0..row.as_ref().column_count() {
                match row.get_ref(index)? {
                    ValueRef::Null => 0_u8.hash(&mut hasher),
                    ValueRef::Integer(value) => (1_u8, value).hash(&mut hasher),
                    ValueRef::Real(value) => (2_u8, value.to_bits()).hash(&mut hasher),
                    ValueRef::Text(bytes) => (3_u8, bytes).hash(&mut hasher),
                    ValueRef::Blob(bytes) => (4_u8, bytes).hash(&mut hasher),
                }
            }
            let hash = hasher.finish();
            let stored = if needs_row(hash) {
                Some(crate::domain::session_row_from_sql(row)?)
            } else {
                None
            };
            Ok((hash, stored))
        })
        .optional()
}

/// CDXC:Coordinators 2026-10-23 WHY:
/// Every two seconds the supervisor rebuilt every thread's row (about 300 on a long-lived install), reading and parsing each session's JSON columns and working out its title, although nearly all of them are resolved or closed threads that cannot change by themselves. A row that depends on neither the clock nor the supervisor's in-memory screen readings (a resolved thread, a session that is gone or not running, with no rename waiting on its agent) is now kept until its thread record or any stored column of its session row changes, or its two-hour `recent` mark runs out. Rows of open running threads are rebuilt every tick as before, so nothing shows later than it did.
fn row_is_stable(thread: &ThreadRecord, session: Option<&Value>) -> bool {
    let Some(session) = session else {
        return true;
    };
    // `project_session_title` shows a waiting rename for eight seconds from its request.
    if crate::presentation::util::read_runtime_text(session, "pendingAgentTitleRequestStatus")
        .as_deref()
        == Some("pending")
    {
        return false;
    }
    thread.is_resolved() || effective_lifecycle_state(session) != "running"
}

fn build_row(
    thread: &ThreadRecord,
    session: Option<&Value>,
    generated_at: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Value {
    let state = classify_thread_session(session, ThreadProgress::of(thread), generated_at, false);
    let detail = match state {
        ThreadState::Waiting => session
            .and_then(waiting_prompt)
            .map(|prompt| report_headline(&prompt.summary, 140))
            .unwrap_or_else(|| "Waiting for an answer".to_string()),
        ThreadState::Working => report_headline(&thread.task, 140),
        _ => thread
            .last_report
            .as_deref()
            .map(|report| report_headline(report, 140))
            .unwrap_or_default(),
    };
    // The session's own activity clock, as the sidebar's relative time reads it; a thread whose
    // session is gone falls back to its record's last change.
    let active_at = session
        .and_then(|session| session.get("lastActiveAt"))
        .and_then(Value::as_str)
        .unwrap_or(thread.updated_at.as_str())
        .to_string();
    let mut row = json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "title": session.map(session_title_of).unwrap_or_else(|| report_headline(&thread.task, 60)),
        "state": state.as_str(),
        "detail": detail,
        "activeAt": active_at,
    });
    if is_recent(&active_at, now) {
        row["recent"] = json!(true);
    }
    if state == ThreadState::Waiting && session.is_some_and(thread_needs_user_approval) {
        row["needsApproval"] = json!(true);
    }
    if let Some(session) = session {
        if let Some(lifecycle) = session.get("lifecycleState") {
            row["lifecycleState"] = lifecycle.clone();
        }
        if let Some(marker) = crate::worktree_sessions::read_worktree_session_marker(session) {
            row["branch"] = json!(marker.branch);
        }
    }
    row
}

/// When `is_recent` turns false for this row, if it is true now.
fn recent_until(row: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    if row["recent"] != true {
        return None;
    }
    let active_at = chrono::DateTime::parse_from_rfc3339(row["activeAt"].as_str()?).ok()?;
    Some(active_at.with_timezone(&chrono::Utc) + chrono::Duration::milliseconds(RECENT_THREAD_MS))
}

fn session_exists(db: &Connection, key: &SessionKey) -> bool {
    db.prepare_cached("SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .and_then(|mut statement| statement.exists([&key.0, &key.1]))
        .unwrap_or(false)
}

/// Rebuilds every coordinator's panel and returns the coordinators whose panel changed.
pub fn refresh_coordinator_panels(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Vec<SessionKey> {
    let Ok(coordinators) = list_coordinators(db) else {
        return Vec::new();
    };
    let threads = list_threads(db).unwrap_or_default();
    let generated_at = now_iso();
    let now = chrono::Utc::now();
    let mut session_rows = db
        .prepare_cached("SELECT * FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .ok();
    let mut previous_rows = rows()
        .lock()
        .map(|mut rows| std::mem::take(&mut *rows))
        .unwrap_or_default();
    let mut next_rows: HashMap<SessionKey, CachedRow> = HashMap::new();
    let mut built: Vec<(SessionKey, Vec<Value>)> = Vec::new();
    for coordinator in coordinators {
        let key = coordinator.key();
        if !session_exists(db, &key) {
            continue;
        }
        let mut panel_rows = Vec::new();
        for thread in threads
            .iter()
            .filter(|thread| thread.coordinator_key() == key)
        {
            let thread_key = thread.key();
            let cached = previous_rows.remove(&thread_key).filter(|cached| {
                cached.thread == *thread && cached.recent_until.is_none_or(|until| now < until)
            });
            // A read error leaves the row uncached and builds it from `None`, as before.
            let stored = session_rows.as_mut().and_then(|statement| {
                read_session_row(statement, &thread_key, |hash| {
                    cached
                        .as_ref()
                        .is_none_or(|cached| cached.session != Some(hash))
                })
                .ok()
            });
            let fingerprint = stored
                .as_ref()
                .map(|row| row.as_ref().map(|(hash, _)| *hash));
            if let (Some(fingerprint), Some(cached)) = (fingerprint, cached) {
                if cached.session == fingerprint {
                    panel_rows.push(cached.row.clone());
                    next_rows.insert(thread_key, cached);
                    continue;
                }
            }
            let session = stored
                .flatten()
                .and_then(|(_, row)| row)
                .and_then(|row| crate::domain::session_from_row(&repository.server_id, row).ok());
            let row = build_row(thread, session.as_ref(), &generated_at, now);
            if let Some(fingerprint) = fingerprint {
                if row_is_stable(thread, session.as_ref()) {
                    next_rows.insert(
                        thread_key,
                        CachedRow {
                            thread: thread.clone(),
                            session: fingerprint,
                            recent_until: recent_until(&row),
                            row: row.clone(),
                        },
                    );
                }
            }
            panel_rows.push(row);
        }
        built.push((key, panel_rows));
    }
    if let Ok(mut rows) = rows().lock() {
        *rows = next_rows;
    }
    let Ok(mut panels) = panels().lock() else {
        return Vec::new();
    };
    let next: HashMap<SessionKey, CachedPanel> = built
        .into_iter()
        .map(|(key, panel_rows)| {
            let panel = cached_panel(panel_rows, panels.get(&key));
            (key, panel)
        })
        .collect();
    let mut changed = next
        .iter()
        .filter(|(key, panel)| {
            panels.get(*key).map(|cached| &cached.summary) != Some(&panel.summary)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    // A coordinator that went away keeps its last panel on any open chat; nothing to republish.
    changed.sort();
    *panels = next;
    changed
}

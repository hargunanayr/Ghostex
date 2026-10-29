//! The Ghost each home project stands for, as the presentation projection publishes it.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;
use serde_json::{json, Value};

use super::records::{list_ghosts, GhostRecord};

/// Home project id → the `ghost` object on that project's presentation row. `None` until first
/// read, so building a snapshot opens the database at most once per gxserver run.
static PUBLISHED_GHOSTS: Mutex<Option<BTreeMap<String, Value>>> = Mutex::new(None);

fn published() -> MutexGuard<'static, Option<BTreeMap<String, Value>>> {
    PUBLISHED_GHOSTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// What a client draws a Ghost from: everything in the row but the timestamps.
pub fn presentation_ghost(ghost: &GhostRecord) -> Value {
    json!({
        "ghostId": ghost.ghost_id,
        "name": ghost.name,
        "mascot": ghost.mascot,
        "color": ghost.color,
        "chatSessionId": ghost.chat_session_id,
        "projectIds": ghost.project_ids,
        "showThreadsInSidebar": ghost.show_threads_in_sidebar,
    })
}

fn read_all(db: &Connection) -> BTreeMap<String, Value> {
    list_ghosts(db)
        .unwrap_or_default()
        .iter()
        .map(|ghost| (ghost.home_project_id.clone(), presentation_ghost(ghost)))
        .collect()
}

/// Re-reads every Ghost after a change, so the next projection carries it.
pub fn refresh_published_ghosts(db: &Connection) {
    *published() = Some(read_all(db));
}

/// The Ghost a home project stands for. The first call of a gxserver run reads the table.
pub fn published_ghost_for_project(project_id: &str) -> Option<Value> {
    let mut published = published();
    if published.is_none() {
        let paths = crate::paths::get_gxserver_paths(None);
        let db = crate::storage::open_gxserver_database(&paths).ok()?;
        *published = Some(read_all(&db));
    }
    published.as_ref()?.get(project_id).cloned()
}

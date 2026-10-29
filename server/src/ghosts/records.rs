//! The `ghosts` table: what only a Ghost has beside its home project and chat session.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

pub(crate) fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite Ghost error: {error}"),
    }
}

pub const GHOST_NAME_MAX_CHARS: usize = 60;
pub const GHOST_MAX_PROJECTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhostRecord {
    pub ghost_id: String,
    pub name: String,
    pub mascot: String,
    pub color: String,
    pub home_project_id: String,
    pub chat_session_id: String,
    /// The projects the Ghost may open threads in, in the order the user added them.
    pub project_ids: Vec<String>,
    pub show_threads_in_sidebar: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl GhostRecord {
    pub fn holds_project(&self, project_id: &str) -> bool {
        self.project_ids.iter().any(|id| id == project_id)
    }

    pub fn to_value(&self) -> Value {
        json!({
            "ghostId": self.ghost_id,
            "name": self.name,
            "mascot": self.mascot,
            "color": self.color,
            "homeProjectId": self.home_project_id,
            "chatSessionId": self.chat_session_id,
            "projectIds": self.project_ids,
            "showThreadsInSidebar": self.show_threads_in_sidebar,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at,
        })
    }
}

const GHOST_COLUMNS: &str = "ghostId, name, mascot, color, homeProjectId, chatSessionId, projectsJson, showThreadsInSidebar, createdAt, updatedAt";

fn parse_project_ids(text: &str) -> Vec<String> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn ghost_from_row(row: &Row<'_>) -> rusqlite::Result<GhostRecord> {
    Ok(GhostRecord {
        ghost_id: row.get(0)?,
        name: row.get(1)?,
        mascot: row.get(2)?,
        color: row.get(3)?,
        home_project_id: row.get(4)?,
        chat_session_id: row.get(5)?,
        project_ids: parse_project_ids(&row.get::<_, String>(6)?),
        show_threads_in_sidebar: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list_ghosts(db: &Connection) -> Result<Vec<GhostRecord>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {GHOST_COLUMNS} FROM ghosts ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], ghost_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

pub fn read_ghost(db: &Connection, ghost_id: &str) -> Result<Option<GhostRecord>, DomainStateError> {
    db.query_row(
        &format!("SELECT {GHOST_COLUMNS} FROM ghosts WHERE ghostId = ?1"),
        params![ghost_id],
        ghost_from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// The Ghost whose home project this is: every session there (its chat, a habit run) is the
/// Ghost's.
pub fn read_ghost_by_home_project(
    db: &Connection,
    project_id: &str,
) -> Result<Option<GhostRecord>, DomainStateError> {
    db.query_row(
        &format!("SELECT {GHOST_COLUMNS} FROM ghosts WHERE homeProjectId = ?1"),
        params![project_id],
        ghost_from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// Whether this session is a Ghost's own chat (not a habit run or a thread).
pub fn is_ghost_chat_session(db: &Connection, project_id: &str, session_id: &str) -> bool {
    read_ghost_by_home_project(db, project_id)
        .ok()
        .flatten()
        .is_some_and(|ghost| ghost.chat_session_id == session_id)
}

pub fn insert_ghost(db: &Connection, ghost: &GhostRecord) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        r#"
        INSERT INTO ghosts (ghostId, name, mascot, color, homeProjectId, chatSessionId, projectsJson, showThreadsInSidebar, createdAt, updatedAt)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        "#,
        params![
            ghost.ghost_id,
            ghost.name,
            ghost.mascot,
            ghost.color,
            ghost.home_project_id,
            ghost.chat_session_id,
            json!(ghost.project_ids).to_string(),
            i64::from(ghost.show_threads_in_sidebar),
            timestamp
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn write_ghost(db: &Connection, ghost: &GhostRecord) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE ghosts
        SET name = ?2, mascot = ?3, color = ?4, chatSessionId = ?5, projectsJson = ?6,
            showThreadsInSidebar = ?7, updatedAt = ?8
        WHERE ghostId = ?1
        "#,
        params![
            ghost.ghost_id,
            ghost.name,
            ghost.mascot,
            ghost.color,
            ghost.chat_session_id,
            json!(ghost.project_ids).to_string(),
            i64::from(ghost.show_threads_in_sidebar),
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn delete_ghost(db: &Connection, ghost_id: &str) -> Result<(), DomainStateError> {
    db.execute("DELETE FROM ghosts WHERE ghostId = ?1", params![ghost_id])
        .map_err(sql_error)?;
    Ok(())
}

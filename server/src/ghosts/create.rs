//! Creating, changing and deleting a Ghost, and keeping its context files current.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::files::{ensure_ghost_home, regenerate_context_files, GhostProjectBrief};
use super::published::refresh_published_ghosts;
use super::records::{
    delete_ghost, insert_ghost, write_ghost, GhostRecord, GHOST_MAX_PROJECTS, GHOST_NAME_MAX_CHARS,
};
use crate::agents::{apply_created_session_identity, create_agent_session_params_for_project};
use crate::coordinators::insert_coordinator;
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

pub struct GhostCreated {
    pub ghost: GhostRecord,
    pub project: Value,
    pub session: Value,
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn clean_name(name: Option<String>) -> Result<String, DomainStateError> {
    let name = name.ok_or_else(|| DomainStateError::bad_request("A Ghost needs a name."))?;
    if name.chars().count() > GHOST_NAME_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Keep a Ghost's name under {GHOST_NAME_MAX_CHARS} characters."
        )));
    }
    Ok(name)
}

/// The projects a request names, each checked to exist and not to be a Ghost's own home.
fn project_ids_param(
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Option<Vec<String>>, DomainStateError> {
    let Some(value) = params.get("projectIds") else {
        return Ok(None);
    };
    let ids = value
        .as_array()
        .ok_or_else(|| DomainStateError::bad_request("projectIds must be a list of project ids."))?;
    let mut out = Vec::new();
    for id in ids.iter().filter_map(Value::as_str).map(str::trim) {
        if id.is_empty() || out.iter().any(|known| known == id) {
            continue;
        }
        if repository.get_project(id)?.is_none() {
            return Err(DomainStateError::not_found(format!(
                "Project {id} does not exist."
            )));
        }
        if super::records::read_ghost_by_home_project(db, id)?.is_some() {
            return Err(DomainStateError::bad_request(
                "A Ghost's home cannot be one of another Ghost's projects.",
            ));
        }
        out.push(id.to_string());
    }
    if out.len() > GHOST_MAX_PROJECTS {
        return Err(DomainStateError::bad_request(format!(
            "A Ghost holds at most {GHOST_MAX_PROJECTS} projects."
        )));
    }
    Ok(Some(out))
}

fn new_ghost_id() -> String {
    format!("gh{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
}

/// CDXC:Ghosts 2026-10-29 DECISION:
/// User chose to build Ghost as a new concept that reuses the Orchestrator's machinery, with Orchestrators staying as they are. So a Ghost's chat session is also a coordinator row (empty goal and instructions; the Soul takes their place): `link_thread`, the supervisor's reports, the Threads panel, the kept title and the Close After Done refusal all work for it unchanged. Its home project is flagged `isGhost` in its launch settings, like a bot's, so no `systemKind` value or table rebuild is needed.
/// SEE-ALSO: server/src/coordinators/ (threads and reports), server/src/server/coordinator_runtime.rs (the supervisor).
pub fn create_ghost(
    paths: &GxserverPaths,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<GhostCreated, DomainStateError> {
    let name = clean_name(text(params, "name"))?;
    let project_ids = project_ids_param(db, repository, params)?.unwrap_or_default();
    let ghost_id = new_ghost_id();
    let home = ensure_ghost_home(paths, &ghost_id)?;
    let project_params = crate::agents::helpers::object_from_value(json!({
        "path": home.to_string_lossy(),
        "name": name,
        "launchSettings": { "isGhost": true, "ghostId": ghost_id },
        "deletedDefaultCommandIds": crate::sidebar_hud::default_sidebar_command_ids()
            .collect::<Vec<_>>(),
    }));
    let project = repository.add_project_path(&project_params)?;
    let home_project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut ghost = GhostRecord {
        ghost_id,
        name: name.clone(),
        mascot: text(params, "mascot").unwrap_or_default(),
        color: text(params, "color").unwrap_or_default(),
        home_project_id: home_project_id.clone(),
        chat_session_id: String::new(),
        project_ids,
        show_threads_in_sidebar: params
            .get("showThreadsInSidebar")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        created_at: String::new(),
        updated_at: String::new(),
    };
    insert_ghost(db, &ghost)?;
    let session = create_chat_session(db, repository, &project, &ghost, params)?;
    ghost.chat_session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    write_ghost(db, &ghost)?;
    refresh_ghost_context(paths, db, repository, &ghost)?;
    Ok(GhostCreated {
        ghost,
        project,
        session,
    })
}

/// A new chat session for the Ghost in its home folder, registered as a coordinator. Used on create
/// and by Reset conversation.
pub fn create_chat_session(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project: &Value,
    ghost: &GhostRecord,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut session_params = Map::new();
    session_params.insert("projectId".to_string(), json!(ghost.home_project_id));
    session_params.insert("title".to_string(), json!(ghost.name));
    for key in ["agentId", "agentModel", "agentEffort", "launchSettings"] {
        if let Some(value) = params.get(key).filter(|value| !value.is_null()) {
            session_params.insert(key.to_string(), value.clone());
        }
    }
    let create_params = create_agent_session_params_for_project(db, project, &session_params)?;
    let created = repository.create_session(&create_params, false)?;
    let session = apply_created_session_identity(repository, &created, &create_params)?;
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    insert_coordinator(db, &ghost.home_project_id, session_id, "", "")?;
    Ok(session)
}

/// The projects a Ghost holds, as its context files name them. A project removed since is left out.
pub fn ghost_project_briefs(
    repository: &DomainRepository<'_>,
    ghost: &GhostRecord,
) -> Vec<GhostProjectBrief> {
    ghost
        .project_ids
        .iter()
        .filter_map(|id| repository.get_project(id).ok().flatten())
        .map(|project| {
            let field = |key: &str| {
                project
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            GhostProjectBrief {
                project_id: field("projectId"),
                name: field("name"),
                path: field("path"),
            }
        })
        .collect()
}

/// Rewrites the context files and republishes the Ghost after any change to it.
pub fn refresh_ghost_context(
    paths: &GxserverPaths,
    db: &Connection,
    repository: &DomainRepository<'_>,
    ghost: &GhostRecord,
) -> Result<(), DomainStateError> {
    regenerate_context_files(paths, ghost, &ghost_project_briefs(repository, ghost))?;
    refresh_published_ghosts(db);
    Ok(())
}

/// `/api/updateGhost`: name, look, projects and the sidebar switch. A rename also renames the home
/// project and the chat session, which show the Ghost's name.
pub fn update_ghost(
    paths: &GxserverPaths,
    db: &Connection,
    repository: &DomainRepository<'_>,
    ghost: &mut GhostRecord,
    params: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    let renamed = match text(params, "name") {
        Some(name) if name != ghost.name => {
            ghost.name = clean_name(Some(name))?;
            true
        }
        _ => false,
    };
    if let Some(mascot) = params.get("mascot").and_then(Value::as_str) {
        ghost.mascot = mascot.trim().to_string();
    }
    if let Some(color) = params.get("color").and_then(Value::as_str) {
        ghost.color = color.trim().to_string();
    }
    if let Some(project_ids) = project_ids_param(db, repository, params)? {
        ghost.project_ids = project_ids;
    }
    if let Some(show) = params.get("showThreadsInSidebar").and_then(Value::as_bool) {
        ghost.show_threads_in_sidebar = show;
    }
    write_ghost(db, ghost)?;
    if renamed {
        let mut project = Map::new();
        project.insert("projectId".to_string(), json!(ghost.home_project_id));
        project.insert("name".to_string(), json!(ghost.name));
        repository.update_project(&project)?;
    }
    refresh_ghost_context(paths, db, repository, ghost)?;
    Ok(renamed)
}

/// Deletes the Ghost, its home project (with its sessions and habits) and its coordinator row. The
/// home folder stays on disk, so a Soul or Memory deleted by mistake can still be recovered.
pub fn delete_ghost_everywhere(
    db: &Connection,
    repository: &DomainRepository<'_>,
    ghost: &GhostRecord,
) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM coordinators WHERE projectId = ?1",
        [&ghost.home_project_id],
    )
    .map_err(super::records::sql_error)?;
    if repository.get_project(&ghost.home_project_id)?.is_some() {
        repository.remove_project(&ghost.home_project_id)?;
    }
    delete_ghost(db, &ghost.ghost_id)?;
    refresh_published_ghosts(db);
    Ok(())
}

//! The Ghost RPCs: list, read, create, update, delete, the Soul and Memory files, and skills.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::artifacts::{
    read_artifact, read_artifact_index, remove_artifact, write_artifact, ArtifactWrite,
};
use super::create::{
    create_chat_session, create_ghost, delete_ghost_everywhere, ghost_project_briefs,
    refresh_ghost_context, update_ghost,
};
use super::files::{
    ghost_file_value, ghost_home_dir, read_ghost_file, write_ghost_file, GHOST_MEMORY_FILE,
    GHOST_SOUL_FILE,
};
use super::memory::{apply_memory_action, list_topics};
use super::records::{list_ghosts, read_ghost, read_ghost_by_home_project, write_ghost, GhostRecord};
use super::skills::{
    add_ghost_skill_from_global, list_ghost_skills, remove_ghost_skill, write_ghost_skill,
};
use crate::coordinators::SessionKey;
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

pub const GHOST_ENDPOINTS: &[&str] = &[
    "/api/listGhosts",
    "/api/readGhost",
    "/api/createGhost",
    "/api/updateGhost",
    "/api/deleteGhost",
    "/api/writeGhostFile",
    "/api/ghostMemory",
    "/api/ghostSkills",
    "/api/ghostArtifacts",
    "/api/resetGhostConversation",
];

/// What an endpoint answered, plus the projects and sessions whose presentation changed.
pub struct GhostEndpointOutput {
    pub result: Value,
    /// `(projectId, deltaType)`: `projectAdded` or `projectUpdated`.
    pub changed_projects: Vec<(String, &'static str)>,
    pub changed_sessions: Vec<SessionKey>,
}

impl GhostEndpointOutput {
    fn result(result: Value) -> Self {
        Self {
            result,
            changed_projects: Vec::new(),
            changed_sessions: Vec::new(),
        }
    }
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The Ghost a request names: `ghostId`, else the Ghost whose home project is `projectId` (any
/// session there, its chat or a habit run, speaks for it).
pub fn require_ghost(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<GhostRecord, DomainStateError> {
    let ghost = match (text(params, "ghostId"), text(params, "projectId")) {
        (Some(ghost_id), _) => read_ghost(db, &ghost_id)?,
        (None, Some(project_id)) => read_ghost_by_home_project(db, &project_id)?,
        (None, None) => {
            return Err(DomainStateError::bad_request(
                "Name the Ghost with ghostId (or the projectId of its home).",
            ))
        }
    };
    ghost.ok_or_else(|| DomainStateError::not_found("That is not a Ghost. List them with ghostex ghost list."))
}

/// A thread a Ghost links must be in one of its projects, or in its own home.
///
/// CDXC:Ghosts 2026-10-29 WHY:
/// The check sits on `/api/linkCoordinatorThread`, the one call every way of starting or adopting a thread goes through (`ghostex ghost start-thread`, `ghostex orchestrator start-thread` and `link`, `ghostex agents create` from the Ghost), so no client can hand a Ghost a project the user did not give it.
pub fn refuse_thread_outside_ghost_projects(
    db: &Connection,
    coordinator_project_id: &str,
    thread_project_id: &str,
) -> Result<(), DomainStateError> {
    let Some(ghost) = read_ghost_by_home_project(db, coordinator_project_id)? else {
        return Ok(());
    };
    if thread_project_id == ghost.home_project_id || ghost.holds_project(thread_project_id) {
        return Ok(());
    }
    Err(DomainStateError::bad_request(format!(
        "{} does not hold that project. Add it in the Ghost's settings (General > Projects) first.",
        ghost.name
    )))
}

fn ghost_view(
    paths: &GxserverPaths,
    repository: &DomainRepository<'_>,
    ghost: &GhostRecord,
    with_files: bool,
) -> Result<Value, DomainStateError> {
    let mut view = ghost.to_value();
    view["homePath"] = json!(ghost_home_dir(paths, &ghost.ghost_id).to_string_lossy());
    view["projects"] = Value::Array(
        ghost_project_briefs(repository, ghost)
            .into_iter()
            .map(|project| {
                json!({ "projectId": project.project_id, "name": project.name, "path": project.path })
            })
            .collect(),
    );
    view["chatSession"] = repository
        .get_session(&ghost.home_project_id, &ghost.chat_session_id)?
        .map(|session| {
            json!({
                "projectId": session["projectId"],
                "sessionId": session["sessionId"],
                "agentId": session["agentId"],
                "title": session["title"],
                "lifecycleState": session["lifecycleState"],
                "launchSettings": session["launchSettings"],
            })
        })
        .unwrap_or(Value::Null);
    if with_files {
        view["soul"] = ghost_file_value(&read_ghost_file(paths, &ghost.ghost_id, GHOST_SOUL_FILE)?);
        view["memory"] =
            ghost_file_value(&read_ghost_file(paths, &ghost.ghost_id, GHOST_MEMORY_FILE)?);
        view["topics"] = json!(list_topics(paths, &ghost.ghost_id));
    }
    Ok(view)
}

pub fn handle_ghost_endpoint(
    endpoint_path: &str,
    paths: &GxserverPaths,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<GhostEndpointOutput, DomainStateError> {
    match endpoint_path {
        "/api/listGhosts" => {
            let ghosts = list_ghosts(db)?
                .iter()
                .map(|ghost| ghost_view(paths, repository, ghost, false))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(GhostEndpointOutput::result(json!({ "ghosts": ghosts })))
        }
        "/api/readGhost" => {
            let ghost = require_ghost(db, params)?;
            Ok(GhostEndpointOutput::result(
                json!({ "ghost": ghost_view(paths, repository, &ghost, true)? }),
            ))
        }
        "/api/createGhost" => {
            let created = create_ghost(paths, db, repository, params)?;
            let session_key = (
                created.ghost.home_project_id.clone(),
                created.ghost.chat_session_id.clone(),
            );
            Ok(GhostEndpointOutput {
                result: json!({
                    "ghost": ghost_view(paths, repository, &created.ghost, false)?,
                    "project": created.project,
                    "session": created.session,
                }),
                changed_projects: vec![(created.ghost.home_project_id.clone(), "projectAdded")],
                changed_sessions: vec![session_key],
            })
        }
        "/api/updateGhost" => {
            let mut ghost = require_ghost(db, params)?;
            let renamed = update_ghost(paths, db, repository, &mut ghost, params)?;
            let mut changed_sessions = Vec::new();
            if renamed {
                let mut session = Map::new();
                session.insert("projectId".to_string(), json!(ghost.home_project_id));
                session.insert("sessionId".to_string(), json!(ghost.chat_session_id));
                session.insert("title".to_string(), json!(ghost.name));
                if repository
                    .get_session(&ghost.home_project_id, &ghost.chat_session_id)?
                    .is_some()
                {
                    repository.update_session(&session)?;
                    changed_sessions.push((ghost.home_project_id.clone(), ghost.chat_session_id.clone()));
                }
            }
            Ok(GhostEndpointOutput {
                result: json!({ "ghost": ghost_view(paths, repository, &ghost, false)? }),
                changed_projects: vec![(ghost.home_project_id.clone(), "projectUpdated")],
                changed_sessions,
            })
        }
        "/api/deleteGhost" => {
            let ghost = require_ghost(db, params)?;
            delete_ghost_everywhere(db, repository, &ghost)?;
            Ok(GhostEndpointOutput {
                result: json!({
                    "ok": true,
                    "homePath": ghost_home_dir(paths, &ghost.ghost_id).to_string_lossy(),
                }),
                changed_projects: vec![(ghost.home_project_id.clone(), "projectUpdated")],
                changed_sessions: Vec::new(),
            })
        }
        "/api/writeGhostFile" => {
            let ghost = require_ghost(db, params)?;
            let file = text(params, "file")
                .ok_or_else(|| DomainStateError::bad_request("Name the file (SOUL.md or MEMORY.md)."))?;
            let body = params.get("text").and_then(Value::as_str).unwrap_or_default();
            let hash = write_ghost_file(
                paths,
                &ghost.ghost_id,
                &file,
                body,
                text(params, "expectedHash").as_deref(),
            )?;
            refresh_ghost_context(paths, db, repository, &ghost)?;
            Ok(GhostEndpointOutput::result(json!({ "ok": true, "hash": hash })))
        }
        "/api/ghostMemory" => {
            let ghost = require_ghost(db, params)?;
            let action = text(params, "action").unwrap_or_else(|| "search".to_string());
            let result = apply_memory_action(
                paths,
                &ghost.ghost_id,
                &action,
                text(params, "fact").as_deref(),
                text(params, "find").as_deref(),
                text(params, "until").as_deref(),
                text(params, "topic").as_deref(),
            )?;
            if action != "search" {
                refresh_ghost_context(paths, db, repository, &ghost)?;
            }
            Ok(GhostEndpointOutput::result(result))
        }
        "/api/ghostSkills" => {
            let ghost = require_ghost(db, params)?;
            let name = text(params, "name").unwrap_or_default();
            match text(params, "action").as_deref().unwrap_or("list") {
                "list" => {}
                "add" => add_ghost_skill_from_global(paths, &ghost.ghost_id, &name)?,
                "write" => write_ghost_skill(
                    paths,
                    &ghost.ghost_id,
                    &name,
                    params.get("skillMd").and_then(Value::as_str).unwrap_or_default(),
                )?,
                "remove" => remove_ghost_skill(paths, &ghost.ghost_id, &name)?,
                _ => {
                    return Err(DomainStateError::bad_request(
                        "Skill actions are list, add, write and remove.",
                    ))
                }
            }
            Ok(GhostEndpointOutput::result(list_ghost_skills(paths, &ghost.ghost_id)))
        }
        "/api/ghostArtifacts" => {
            let ghost = require_ghost(db, params)?;
            let id = text(params, "id");
            let result = match text(params, "action").as_deref().unwrap_or("list") {
                "list" => json!({ "artifacts": read_artifact_index(paths, &ghost.ghost_id) }),
                "read" => json!({
                    "artifact": read_artifact(paths, &ghost.ghost_id, id.as_deref().unwrap_or_default())?
                }),
                "write" => {
                    let entry = write_artifact(
                        paths,
                        &ghost.ghost_id,
                        &ArtifactWrite {
                            id: id.as_deref(),
                            kind: text(params, "kind").as_deref().unwrap_or("markdown"),
                            title: text(params, "title").as_deref().unwrap_or_default(),
                            summary: text(params, "summary").as_deref().unwrap_or_default(),
                            text: params.get("text").and_then(Value::as_str).unwrap_or_default(),
                            visual_page_id: text(params, "visualPageId").as_deref(),
                            url: text(params, "url").as_deref(),
                        },
                    )?;
                    json!({ "artifact": entry })
                }
                "remove" => {
                    remove_artifact(paths, &ghost.ghost_id, id.as_deref().unwrap_or_default())?;
                    json!({ "ok": true })
                }
                _ => {
                    return Err(DomainStateError::bad_request(
                        "Document actions are list, read, write and remove.",
                    ))
                }
            };
            Ok(GhostEndpointOutput::result(result))
        }
        "/api/resetGhostConversation" => {
            let mut ghost = require_ghost(db, params)?;
            let project = repository
                .get_project(&ghost.home_project_id)?
                .ok_or_else(|| DomainStateError::not_found("The Ghost's home project is gone."))?;
            // The new chat keeps the old one's agent and launch settings unless the request names
            // others.
            let mut session_params = params.clone();
            if let Some(old) =
                repository.get_session(&ghost.home_project_id, &ghost.chat_session_id)?
            {
                for key in ["agentId", "launchSettings"] {
                    if !session_params.contains_key(key) {
                        if let Some(value) = old.get(key).filter(|value| !value.is_null()) {
                            session_params.insert(key.to_string(), value.clone());
                        }
                    }
                }
            }
            db.execute(
                "DELETE FROM coordinators WHERE projectId = ?1 AND sessionId = ?2",
                [&ghost.home_project_id, &ghost.chat_session_id],
            )
            .map_err(super::records::sql_error)?;
            let old_session = ghost.chat_session_id.clone();
            let session = create_chat_session(db, repository, &project, &ghost, &session_params)?;
            ghost.chat_session_id = session
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            write_ghost(db, &ghost)?;
            refresh_ghost_context(paths, db, repository, &ghost)?;
            Ok(GhostEndpointOutput {
                result: json!({ "ghost": ghost_view(paths, repository, &ghost, false)?, "session": session }),
                changed_projects: vec![(ghost.home_project_id.clone(), "projectUpdated")],
                changed_sessions: vec![
                    (ghost.home_project_id.clone(), old_session),
                    (ghost.home_project_id.clone(), ghost.chat_session_id.clone()),
                ],
            })
        }
        other => Err(DomainStateError::not_found(format!(
            "{other} is not a Ghost endpoint."
        ))),
    }
}

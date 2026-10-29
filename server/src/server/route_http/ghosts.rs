//! Ghost routes: the rules live in `crate::ghosts`; this publishes what each call changed.

use anyhow::Result;

use super::*;

pub(super) async fn route_ghosts_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if !crate::ghosts::GHOST_ENDPOINTS.contains(&request.endpoint.path.as_str()) {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    let endpoint_path = endpoint.path.clone();
    if matches!(
        endpoint_path.as_str(),
        "/api/deleteGhost" | "/api/resetGhostConversation"
    ) {
        let closer = state.clone();
        let body = body_json.clone();
        let deleting = endpoint_path == "/api/deleteGhost";
        let _ = tokio::task::spawn_blocking(move || {
            close_ghost_sessions(&closer, &body, deleting)
        })
        .await;
    }
    Ok(handle_domain_http(
        &state,
        endpoint.path,
        request_id,
        &body_json,
        |repository, db, params, _| {
            let output = crate::ghosts::handle_ghost_endpoint(
                &endpoint_path,
                &state.paths,
                db,
                repository,
                params,
            )?;
            for (project_id, delta_type) in &output.changed_projects {
                schedule_presentation_project_delta(&state, db, repository, project_id, delta_type)?;
            }
            for (project_id, session_id) in &output.changed_sessions {
                if repository.get_session(project_id, session_id)?.is_some() {
                    schedule_presentation_session_delta(
                        &state, db, repository, project_id, session_id,
                    )?;
                }
            }
            Ok(output.result)
        },
    ))
}

/// Closes the agents a Ghost leaves behind before its rows go: every session of its home (its chat
/// and any habit run) when it is deleted, its old chat when its conversation is reset. Without this
/// their daemons would keep running with no row to reach them from.
fn close_ghost_sessions(state: &AppState, body: &Value, every_session: bool) {
    let Ok(params) = crate::domain::read_domain_rpc_params(body) else {
        return;
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let Ok(ghost) = crate::ghosts::require_ghost(&db, &params) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let sessions = if every_session {
        repository
            .list_sessions(Some(&ghost.home_project_id))
            .unwrap_or_default()
            .iter()
            .filter_map(|session| session["sessionId"].as_str().map(str::to_string))
            .collect::<Vec<_>>()
    } else {
        vec![ghost.chat_session_id.clone()]
    };
    drop(repository);
    drop(db);
    for session_id in sessions {
        let mut close = Map::new();
        close.insert("action".into(), json!("close"));
        close.insert("projectId".into(), json!(ghost.home_project_id));
        close.insert("sessionId".into(), json!(session_id));
        close.insert("reason".into(), json!("ghostRemoved"));
        let _ = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/transitionSession".to_string(),
            "ghost-close".to_string(),
            close,
        );
    }
}

//! Opening the Ghost modal (New Ghost from the sidebar's +, a Ghost's settings from its gear) and
//! carrying out what it asks: every save is one Ghost RPC on this computer's gxserver, and its
//! answer goes back to the modal, which stays open until it closes itself.

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::*;
use ghostex_gx_core::{
    ProjectPlace, local_agent_launch_params, resolve_sidebar_agent, start_provider_params,
};
use gpui::{AppContext as _, Window};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The projects a Ghost can hold: every project of this computer but Ghost homes, bots and
/// projects closed to Recent. Grouped ones come first, by project group in sidebar order; the rest
/// follow by name.
fn holdable_projects(listed: &Value, places: &HashMap<String, ProjectPlace>) -> Vec<GhostModalProject> {
    let mut projects = listed["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|project| {
            let launch = &project["launchSettings"];
            launch["isGhost"].as_bool() != Some(true)
                && launch["isBot"].as_bool() != Some(true)
                && project["isRecentProject"].as_bool() != Some(true)
        })
        .filter_map(|project| {
            let project_id = project["projectId"].as_str()?.to_string();
            let place = places.get(&project_id).cloned().unwrap_or_default();
            let ranked = !place.group.is_empty();
            Some((
                (
                    if ranked { place.group_rank } else { usize::MAX },
                    if ranked { place.rank } else { 0 },
                ),
                GhostModalProject {
                    project_id,
                    name: project["name"].as_str().unwrap_or_default().to_string(),
                    group: place.group,
                    group_color: place.group_color,
                },
            ))
        })
        .collect::<Vec<_>>();
    projects.sort_by_key(|(order, project)| (*order, project.name.to_lowercase()));
    projects.into_iter().map(|(_, project)| project).collect()
}

impl GhostexGpuiApp {
    /// The project group of every local project, by project id, whatever the sidebar is showing.
    fn ghost_modal_sidebar_places(&self) -> HashMap<String, ProjectPlace> {
        self.gx_store
            .sidebar_list
            .project_places(&self.gx_store.core)
            .into_iter()
            .map(|place| (place.project_id.clone(), place))
            .collect()
    }

    /// The agents a Ghost can run on: the ones an orchestrator can, Claude first.
    fn ghost_modal_agents(&self) -> Vec<GhostModalAgent> {
        self.gx_store_coordinator_agents()
            .into_iter()
            .map(|agent| GhostModalAgent {
                agent_id: agent.agent_id,
                name: agent.name,
            })
            .collect()
    }

    /// The sidebar's + on the Ghosts section.
    pub(crate) fn open_new_ghost_modal(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let agents = self.ghost_modal_agents();
        cx.spawn(async move |this, cx| {
            let listed = gx_rpc(None, "/api/listProjects", json!({})).await.unwrap_or_default();
            let _ = this.update(cx, |app, cx| {
                let config = GhostModalConfig {
                    ghost: None,
                    skills: Value::Null,
                    agents,
                    projects: holdable_projects(&listed, &app.ghost_modal_sidebar_places()),
                    palette: app.gpui_native_modal_palette(),
                };
                app.open_ghost_modal_window(config, None, cx);
            });
        })
        .detach();
    }

    /// A Ghost row's gear, and the Overview's settings button.
    pub(crate) fn open_ghost_settings_modal(
        &mut self,
        ghost_id: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let ghost_id = ghost_id.to_string();
        cx.spawn(async move |this, cx| {
            let ghost = gx_rpc(None, "/api/readGhost", json!({ "ghostId": ghost_id })).await;
            let skills = gx_rpc(
                None,
                "/api/ghostSkills",
                json!({ "ghostId": ghost_id, "action": "list" }),
            )
            .await
            .unwrap_or_default();
            let listed = gx_rpc(None, "/api/listProjects", json!({})).await.unwrap_or_default();
            let _ = this.update(cx, |app, cx| {
                let ghost = match ghost {
                    Ok(value) => value["ghost"].clone(),
                    Err(error) => {
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Ghost settings did not open",
                            &error.message,
                            cx,
                        );
                        return;
                    }
                };
                let config = GhostModalConfig {
                    ghost: Some(ghost),
                    skills,
                    agents: Vec::new(),
                    projects: holdable_projects(&listed, &app.ghost_modal_sidebar_places()),
                    palette: app.gpui_native_modal_palette(),
                };
                app.open_ghost_modal_window(config, Some(ghost_id), cx);
            });
        })
        .detach();
    }

    fn open_ghost_modal_window(
        &mut self,
        config: GhostModalConfig,
        ghost_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.run_ghost_modal_command(ghost_id.clone(), command, cx)
        });
        self.open_native_app_modal(
            GpuiAppModalKind::Ghost,
            GHOST_MODAL_WIDTH,
            GHOST_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiGhostModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    fn run_ghost_modal_command(
        &mut self,
        ghost_id: Option<String>,
        command: GhostModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let ghost = json!(ghost_id);
        let (path, params, reply) = match command {
            GhostModalCommand::Cancel => {
                self.release_native_app_modal_window(GpuiAppModalKind::Ghost, cx);
                return;
            }
            GhostModalCommand::Create { name, mascot, color, agent_id, project_ids, reply } => {
                let hud = self.gx_store_launch_hud();
                let Some(agent) = resolve_sidebar_agent(hud.as_deref(), &agent_id) else {
                    reply.finish(Err("That agent is no longer configured.".to_string()), cx);
                    return;
                };
                let mut params = local_agent_launch_params(&agent, "", Map::new(), None, &name);
                params["name"] = json!(name);
                params["mascot"] = json!(mascot);
                params["color"] = json!(color);
                params["projectIds"] = json!(project_ids);
                if let Some(object) = params.as_object_mut() {
                    object.remove("projectId");
                }
                self.create_ghost_from_modal(params, reply, cx);
                return;
            }
            GhostModalCommand::SaveGeneral { name, mascot, color, project_ids, reply } => (
                "/api/updateGhost",
                json!({
                    "ghostId": ghost,
                    "name": name,
                    "mascot": mascot,
                    "color": color,
                    "projectIds": project_ids,
                }),
                reply,
            ),
            GhostModalCommand::SaveFile { file, text, expected_hash, reply } => (
                "/api/writeGhostFile",
                json!({ "ghostId": ghost, "file": file, "text": text, "expectedHash": expected_hash }),
                reply,
            ),
            GhostModalCommand::Skill { action, name, reply } => (
                "/api/ghostSkills",
                json!({ "ghostId": ghost, "action": action, "name": name }),
                reply,
            ),
            GhostModalCommand::Reset { reply } => (
                "/api/resetGhostConversation",
                json!({ "ghostId": ghost }),
                reply,
            ),
            GhostModalCommand::Delete { reply } => {
                ("/api/deleteGhost", json!({ "ghostId": ghost }), reply)
            }
        };
        let deleting = path == "/api/deleteGhost";
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, path, params)
                .await
                .map_err(|error| error.message);
            let _ = this.update(cx, |app, cx| {
                let succeeded = result.is_ok();
                reply.finish(result, cx);
                if succeeded {
                    if deleting {
                        app.release_native_app_modal_window(GpuiAppModalKind::Ghost, cx);
                        if app.active_mode == crate::app::model::TitlebarMode::GhostOverview {
                            app.ghost_overview_ghost_id = None;
                        }
                    }
                    app.native_ghost_overview_reload(cx);
                }
            });
        })
        .detach();
    }

    /// `/api/createGhost`, then starts its chat and opens it, like a new orchestrator.
    fn create_ghost_from_modal(
        &mut self,
        params: Value,
        reply: GhostModalReply,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(None, "/api/createGhost", params)
                .await
                .map_err(|error| error.message);
            let session = created.as_ref().ok().map(|created| {
                (
                    created["ghost"]["ghostId"].as_str().unwrap_or_default().to_string(),
                    created["session"]["projectId"].as_str().unwrap_or_default().to_string(),
                    created["session"]["sessionId"].as_str().unwrap_or_default().to_string(),
                )
            });
            if let Some((_, project_id, session_id)) = &session {
                let _ = gx_rpc(
                    None,
                    "/api/startSessionProvider",
                    start_provider_params(project_id, session_id),
                )
                .await;
            }
            let _ = this.update(cx, |app, cx| {
                let succeeded = created.is_ok();
                reply.finish(created, cx);
                if succeeded {
                    app.release_native_app_modal_window(GpuiAppModalKind::Ghost, cx);
                }
                if let Some((ghost_id, project_id, session_id)) = session {
                    app.open_ghost(&ghost_id, &project_id, &session_id, cx);
                }
            });
        })
        .detach();
    }
}

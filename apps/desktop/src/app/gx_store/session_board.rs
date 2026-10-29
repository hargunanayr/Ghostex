//! The store side of the Board: rebuilding its lanes from the sidebar's own store and inputs when
//! the sessions move, and carrying its buttons to the card's machine. The rules are gx-core's
//! (`session_board/`) and gxserver's (`session_board/`).

use std::time::Duration;

use ghostex_gx_core::session_board::{
    BoardCard, SessionBoardView, build_session_board, close_completed_message,
    complete_all_params, complete_params, read_task_params, start_task_params,
    update_task_params, with_board_task, COMPLETE_SESSIONS_ENDPOINT, COMPLETE_SESSION_ENDPOINT,
    READ_SESSION_TASK_ENDPOINT, START_SESSION_TASK_ENDPOINT, UPDATE_SESSION_TASK_ENDPOINT,
};
use ghostex_gx_core::{
    MachineId, first_prompt_title_runtime_settings, local_agent_launch_params,
    remote_agent_launch_params, remote_launch_agent_id, resolve_sidebar_agent,
};
use serde_json::{Value, json};

use super::gx_rpc;
use super::host::now_ms;
use crate::GhostexGpuiApp;

/// Session changes arrive in bursts (a working agent publishes several a second), so the Board
/// rebuilds at most this often while it shows.
///
/// CDXC:SessionBoard 2026-10-28 WHY: A rebuild is a whole sidebar list built without its cache (the Space's set, not the drawn list), so it is coalesced instead of run on every store change, and skipped while the Board is not showing: opening it rebuilds it.
const REBUILD_COALESCE: Duration = Duration::from_millis(250);

/// One agent a task can run, for the Add task panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoardAgentOption {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    /// The models a task on this agent can start on; empty when its launch takes no model choice.
    pub(crate) models: Vec<BoardModelOption>,
    /// The efforts the agent knows, for a task left on its default model.
    pub(crate) efforts: Vec<(String, String)>,
}

/// One model of the Add task panel's Model menu, with the efforts (id, label) it accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoardModelOption {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) efforts: Vec<(String, String)>,
}

/// The agent families whose launch takes a model and an effort for a thread (gxserver's
/// `apply_requested_agent_model`). ZCode takes a model for an orchestrator only, and Empryo seeds a
/// session folder instead, so neither is offered here.
///
/// CDXC:SessionBoard 2026-10-28 DECISION:
/// User (Ryan): Add task offers a model and an effort, like PandaCode's "Default · Claude Opus 5.5 High". Default leaves the choice to the agent's remembered model; a pick is that task's own (`agentModel` / `agentEffort` on the create, never taught to the default).
const MODEL_CHOICE_FAMILIES: [&str; 4] = ["claude", "codex", "pi", "antigravity"];

/// The model lineup and efforts for `agent` from the newest catalog this computer has.
fn board_agent_models(
    agent_id: &str,
    icon: Option<&str>,
) -> (Vec<BoardModelOption>, Vec<(String, String)>) {
    let family = ghostex_gx_core::default_agent_id_for_icon(icon)
        .or_else(|| ghostex_gx_core::default_agent_id_for_icon(Some(agent_id)))
        .unwrap_or(agent_id);
    if !MODEL_CHOICE_FAMILIES.contains(&family) {
        return (Vec::new(), Vec::new());
    }
    let Some(catalog) = crate::app::gx_chat::current_model_catalog() else {
        return (Vec::new(), Vec::new());
    };
    let Some(lineup) = catalog.agents.get(family) else {
        return (Vec::new(), Vec::new());
    };
    let labelled = |efforts: &[String]| {
        efforts
            .iter()
            .map(|effort| (effort.clone(), catalog.effort_label(effort)))
            .collect::<Vec<_>>()
    };
    let models = lineup
        .models
        .iter()
        .map(|model| BoardModelOption {
            value: model.value.clone(),
            label: model.label.clone(),
            efforts: labelled(&model.efforts),
        })
        .collect();
    (models, labelled(&lineup.efforts))
}

impl GhostexGpuiApp {
    /// The sidebar list moved (`gx_store_update_sidebar_list`).
    pub(crate) fn gx_store_session_board_store_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if self.native_session_board.view.is_none() {
            return;
        }
        if !self.session_board_showing() {
            self.native_session_board.stale = true;
            return;
        }
        if self.native_session_board.rebuild_pending {
            return;
        }
        self.native_session_board.rebuild_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REBUILD_COALESCE).await;
            let _ = this.update(cx, |this, cx| {
                this.native_session_board.rebuild_pending = false;
                this.rebuild_native_session_board(cx);
            });
        })
        .detach();
    }

    /// Builds the lanes now from the store and the sidebar's inputs, with the filter the page holds.
    pub(crate) fn rebuild_native_session_board(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(view) = self.native_session_board.view.clone() else {
            return;
        };
        self.native_session_board.stale = false;
        let filter = view.read(cx).filter().clone();
        let board = build_session_board(
            &self.gx_store.core,
            &self.gx_store.sidebar_list.last_inputs,
            &filter,
            now_ms(),
        );
        // The chat panel's header reads the card as the store has it now; a card the filter hides
        // keeps its last reading.
        if let Some(chat) = self.native_session_board.chat.as_mut()
            && let Some(card) = board.card(&chat.sidebar_session_id)
            && card != chat
        {
            *chat = card.clone();
            cx.notify();
        }
        view.update(cx, |view, cx| view.set_board(board, cx));
    }

    /// The agents Add task offers, from the launcher's list, and the one it starts on.
    pub(crate) fn session_board_agent_options(&self) -> (Vec<BoardAgentOption>, Option<String>) {
        let hud = self.gx_store_launch_hud();
        let agents: Vec<BoardAgentOption> = hud
            .as_deref()
            .and_then(|hud| hud.get("agents"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|agent| agent.get("agentId").and_then(Value::as_str))
            .filter_map(|agent_id| resolve_sidebar_agent(hud.as_deref(), agent_id))
            .filter(|agent| agent.launch_command().is_some())
            .map(|agent| {
                let (models, efforts) = board_agent_models(&agent.agent_id, agent.icon.as_deref());
                BoardAgentOption {
                    agent_id: agent.agent_id,
                    name: agent.name,
                    icon: agent.icon,
                    models,
                    efforts,
                }
            })
            .collect();
        let preferred = hud
            .as_deref()
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| agents.iter().any(|agent| agent.agent_id == *id))
            .map(str::to_string)
            .or_else(|| agents.first().map(|agent| agent.agent_id.clone()));
        (agents, preferred)
    }

    fn session_board_rpc(
        &self,
        machine: &MachineId,
        path: &'static str,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<Value, String>> {
        let remote = match machine.remote_id() {
            None => None,
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    return gpui::Task::ready(Err(
                        "That machine is not connected right now.".to_string()
                    ));
                }
            },
        };
        cx.background_executor().spawn(async move {
            gx_rpc(remote, path, params)
                .await
                .map_err(|error| error.message)
        })
    }

    fn session_board_toast(&mut self, title: &str, detail: &str, cx: &mut gpui::Context<Self>) {
        self.receive_gpui_app_toast_bridge_message(
            &json!({ "level": "warning", "title": title, "description": detail, "type": "toast" }),
            cx,
        );
    }

    /// Runs `task` and says why it failed, if it did; the store update it causes redraws the Board.
    fn session_board_report(
        &mut self,
        title: &'static str,
        task: gpui::Task<Result<Value, String>>,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                let _ = this.update(cx, |this, cx| this.session_board_toast(title, &error, cx));
            }
        })
        .detach();
    }

    /// Complete on a Review card, or Reopen on a Completed one.
    pub(crate) fn session_board_set_completed(
        &mut self,
        card: &BoardCard,
        completed: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let task = self.session_board_rpc(
            &card.key.machine,
            COMPLETE_SESSION_ENDPOINT,
            complete_params(&card.key, completed),
            cx,
        );
        let title = if completed {
            "Could not complete the thread"
        } else {
            "Could not reopen the thread"
        };
        self.session_board_report(title, task, cx);
    }

    /// Complete all: the Review cards the filter shows, minus those waiting on an answer.
    pub(crate) fn session_board_complete_all(
        &mut self,
        board: &SessionBoardView,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(params) = complete_all_params(board) else {
            return;
        };
        let Some(machine) = board
            .lanes
            .iter()
            .flat_map(|lane| &lane.cards)
            .next()
            .map(|card| card.key.machine.clone())
        else {
            return;
        };
        let task = self.session_board_rpc(&machine, COMPLETE_SESSIONS_ENDPOINT, params, cx);
        self.session_board_report("Could not complete the threads", task, cx);
    }

    /// Close all: the sidebar's own bulk close of the Completed cards the filter shows.
    pub(crate) fn session_board_close_completed(
        &mut self,
        board: &SessionBoardView,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(message) = close_completed_message(board) {
            self.dispatch_native_sidebar_command(message, cx);
        }
    }

    /// Delete on a task: the thread is closed like any other.
    pub(crate) fn session_board_close_thread(
        &mut self,
        card: &BoardCard,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_native_sidebar_command(
            json!({ "type": "closeSessions", "sessionIds": [card.sidebar_session_id] }),
            cx,
        );
    }

    pub(crate) fn session_board_start_task(
        &mut self,
        card: &BoardCard,
        cx: &mut gpui::Context<Self>,
    ) {
        let task = self.session_board_rpc(
            &card.key.machine,
            START_SESSION_TASK_ENDPOINT,
            start_task_params(&card.key),
            cx,
        );
        self.session_board_report("Could not start the task", task, cx);
    }

    /// The task's whole prompt, for its edit panel.
    pub(crate) fn session_board_read_task(
        &mut self,
        card: &BoardCard,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<Value, String>> {
        self.session_board_rpc(
            &card.key.machine,
            READ_SESSION_TASK_ENDPOINT,
            read_task_params(&card.key),
            cx,
        )
    }

    /// Saves a waiting task's title and prompt; `then_start` sends it once saved.
    pub(crate) fn session_board_save_task(
        &mut self,
        card: &BoardCard,
        title: Option<&str>,
        prompt: &str,
        then_start: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<Value, String>> {
        let save = self.session_board_rpc(
            &card.key.machine,
            UPDATE_SESSION_TASK_ENDPOINT,
            update_task_params(&card.key, title, prompt),
            cx,
        );
        if !then_start {
            return save;
        }
        let start = self.session_board_rpc(
            &card.key.machine,
            START_SESSION_TASK_ENDPOINT,
            start_task_params(&card.key),
            cx,
        );
        cx.background_executor().spawn(async move {
            save.await?;
            start.await
        })
    }

    /// Add task: a draft of `agent_id` in `project_id` on the machine the Board shows, holding the
    /// prompt and not started, so it waits in Pending. `model` and `effort` are the task's own
    /// launch choice; `None` leaves the agent's default.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn session_board_create_task(
        &mut self,
        project_id: &str,
        agent_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
        title: Option<&str>,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<Value, String>> {
        let machine = match self.gx_store.sidebar_list.last_inputs.ui.selected_machine_id.as_str() {
            ghostex_gx_core::LOCAL_MACHINE_ID => MachineId::Local,
            machine_id => MachineId::Remote(machine_id.to_string()),
        };
        let hud = self.gx_store_launch_hud();
        let title_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            None,
        );
        let base = if machine.is_local() {
            let Some(agent) = resolve_sidebar_agent(hud.as_deref(), agent_id) else {
                return gpui::Task::ready(Err("That agent is not available.".to_string()));
            };
            local_agent_launch_params(&agent, project_id, title_settings, None, "")
        } else {
            remote_agent_launch_params(
                &remote_launch_agent_id(hud.as_deref(), agent_id),
                project_id,
                title_settings,
                None,
                "",
            )
        };
        let mut params = with_board_task(base, prompt, title);
        if let Some(object) = params.as_object_mut() {
            if let Some(model) = model.filter(|model| !model.is_empty()) {
                object.insert("agentModel".to_string(), json!(model));
            }
            if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
                object.insert("agentEffort".to_string(), json!(effort));
            }
        }
        self.session_board_rpc(&machine, "/api/createAgentSession", params, cx)
    }
}

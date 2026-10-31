//! Open, close, and bridge plumbing for the native Rename Session dialog.
//! SEE-ALSO: apps/desktop/src/app/window/rename_session_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;
use std::path::PathBuf;

/// GPUI-owned replacement for the React host's `ghostex.promptAgent.renameSession` localStorage key.
pub(crate) fn gpui_rename_session_modal_prefs_path() -> PathBuf {
    ghostex_state_root().join("gpui-rename-session-modal.json")
}

/// `sidebarAgentIconSupportsSessionHistoryTitleGeneration`: the agents whose
/// transcript gxserver can summarize into a name.
fn gpui_session_agent_icon_supports_history_title_generation(icon: Option<&str>) -> bool {
    matches!(
        icon,
        Some("claude" | "codex" | "cursor-cli" | "antigravity-cli" | "empryo")
    )
}

/// The `hud.agents` rows with a launch command, the filter the React dialog applies.
fn gpui_rename_session_prompt_agents(hud: Option<&serde_json::Value>) -> Vec<RenameSessionAgent> {
    hud.and_then(|hud| hud.get("agents"))
        .and_then(serde_json::Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter_map(|value| {
                    let field = |key: &str| {
                        value
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_string)
                    };
                    field("command")?;
                    Some(RenameSessionAgent {
                        agent_id: field("agentId")?,
                        name: field("name")?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the `open` message of the `renameSession`
    /// modal kind: `sessionId` (required), `initialTitle`, `sessionAgentIcon`,
    /// and the agent list and default prompt agent from `latestSidebarStateMessage.hud`.
    pub(crate) fn open_gpui_rename_session_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = message
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let initial_title = message
            .get("initialTitle")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let can_generate_from_history = gpui_session_agent_icon_supports_history_title_generation(
            message
                .get("sessionAgentIcon")
                .and_then(serde_json::Value::as_str),
        );
        let hud = message
            .get("latestSidebarStateMessage")
            .and_then(|state| state.get("hud"));
        let default_prompt_agent_id = hud
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|agent_id| !agent_id.is_empty())
            .map(str::to_string);
        let config = RenameSessionModalConfig {
            agents: gpui_rename_session_prompt_agents(hud),
            default_prompt_agent_id,
            initial_title,
            can_generate_from_history,
            palette: self.gpui_native_dialog_palette(),
            prefs_path: Some(gpui_rename_session_modal_prefs_path()),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_rename_session_modal_command(&session_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RenameSession,
            RENAME_SESSION_MODAL_WIDTH,
            RENAME_SESSION_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiRenameSessionModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Posts the same `renameSession` command the React page did, then
    /// releases the window the dialog already removed.
    fn handle_gpui_rename_session_modal_command(
        &mut self,
        session_id: &str,
        command: RenameSessionModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::RenameSession;
        let mut message = serde_json::Map::new();
        message.insert("sessionId".to_string(), serde_json::json!(session_id));
        message.insert("type".to_string(), serde_json::json!("renameSession"));
        match command {
            RenameSessionModalCommand::Rename { title } => {
                message.insert("title".to_string(), serde_json::json!(title));
                self.handle_gpui_rename_command_session_command(&message, cx);
            }
            RenameSessionModalCommand::GenerateName { title, agent_id } => {
                if let Some(agent_id) = agent_id {
                    message.insert("agentId".to_string(), serde_json::json!(agent_id));
                }
                message.insert("shouldGenerateTitle".to_string(), serde_json::json!(true));
                message.insert("title".to_string(), serde_json::json!(title));
                self.handle_gpui_rename_command_session_command(&message, cx);
            }
            RenameSessionModalCommand::Cancel => {}
        }
        self.release_native_app_modal_window(kind, cx);
    }
}

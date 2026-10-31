//! Where a new session starts and how a draft changes project: the choices the new chat's tray
//! offers on any sidebar, the Unified header's New session, and the move itself.

use ghostex_gx_core::unified_sidebar::{
    ProjectChoiceInput, UnifiedChoiceMachineInput, UnifiedProjectChoice,
    unified_new_session_target, unified_project_choices,
};
use ghostex_gx_core::{LOCAL_MACHINE_ID, MachineId, ProjectKey};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// A draft on its way to another project: what starts there, and what closes once it has.
#[derive(Clone, Debug)]
pub(crate) struct PendingDraftMove {
    pub(crate) from_machine_id: String,
    pub(crate) from_project_id: String,
    pub(crate) from_session_id: String,
    /// What the composer held, which the new chat opens with.
    pub(crate) text: String,
}

fn group_id_of(machine_id: &str, project_id: &str) -> String {
    let machine = if machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    };
    ProjectKey {
        machine,
        project_id: project_id.to_string(),
    }
    .to_sidebar_group_id()
}

/// The params of `createAgentSession` with the moved draft's text staged for its chat, the same way
/// `ghostex create-agent --first-input-draft` does.
pub(in crate::app) fn with_moved_draft_text(
    mut params: Value,
    draft: &Option<PendingDraftMove>,
) -> Value {
    if let Some(draft) = draft.as_ref().filter(|draft| !draft.text.trim().is_empty()) {
        params["runtimeSettings"]["firstUserInputDraft"] = json!(draft.text);
    }
    params
}

impl GhostexGpuiApp {
    /// One row per repository across the machines this app holds, for the tray and the "+" of a
    /// group. It reads the store directly, so it answers with the Project sidebar selected too.
    pub(crate) fn gx_store_project_choices(&self) -> Vec<UnifiedProjectChoice> {
        let inputs = &self.gx_store.sidebar_list.last_inputs;
        let store = self.gx_store.core.presentation();
        let mut machines: Vec<(MachineId, String, bool)> =
            vec![(MachineId::Local, "Local".to_string(), true)];
        for tab in &inputs.host.machines {
            if tab.machine_id == LOCAL_MACHINE_ID {
                machines[0].1.clone_from(&tab.label);
            } else if tab.fed && !tab.this_computer {
                machines.push((
                    MachineId::Remote(tab.machine_id.clone()),
                    tab.label.clone(),
                    tab.state == "connected",
                ));
            }
        }
        let hidden = &inputs.ui.hidden_items.group_ids;
        let machine_inputs: Vec<UnifiedChoiceMachineInput> = machines
            .into_iter()
            .filter_map(|(machine, label, connected)| {
                let projects = store
                    .loaded(&machine)?
                    .projects()
                    .iter()
                    .filter(|project| {
                        inputs.ui.show_hidden
                            || !hidden.contains(
                                &ProjectKey {
                                    machine: machine.clone(),
                                    project_id: project.project_id.clone(),
                                }
                                .to_sidebar_group_id(),
                            )
                    })
                    .filter_map(|project| {
                        Some(ProjectChoiceInput {
                            project_id: project.project_id.clone(),
                            title: project.title.clone(),
                            path: project.path.clone()?,
                            git_remote_origin_url: project.git_remote_origin_url.value().cloned(),
                            worktree_parent_path: project
                                .worktree
                                .as_ref()
                                .and_then(|worktree| worktree.get("parentProjectPath"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect();
                Some(UnifiedChoiceMachineInput {
                    machine,
                    label,
                    connected,
                    projects,
                })
            })
            .collect();
        unified_project_choices(
            &machine_inputs,
            &self.gx_store.unified_sidebar.saved.last_machines,
            &inputs.ui.selected_machine_id,
        )
    }

    /// The header's New session button: the picker, aimed at the filtered repository or the active
    /// project. Nothing is activated, so it cannot start a default agent (CDXC:Sidebar 2026-10-31
    /// in gx-core unified_sidebar/choices.rs).
    pub(crate) fn open_unified_new_session(&mut self, cx: &mut gpui::Context<Self>) {
        let choices = self.gx_store_project_choices();
        let active = self.gpui_app_modal_active_project_id();
        let target = unified_new_session_target(
            &self.gx_store.unified_sidebar.filter,
            &choices,
            active.as_deref(),
        );
        self.gx_store.unified_sidebar.picker_target =
            target.map(|target| group_id_of(&target.machine_id, &target.project_id));
        self.open_unified_new_session_picker(cx);
    }

    /// A group's "+": the picker in that project.
    pub(crate) fn open_new_session_in(
        &mut self,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.unified_sidebar.picker_target = Some(group_id_of(machine_id, project_id));
        self.open_unified_new_session_picker(cx);
    }

    fn open_unified_new_session_picker(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(not(target_arch = "wasm32"))]
        self.open_gpui_new_thread_picker(cx);
        // The page has no picker window: a terminal session, as its New session always made.
        #[cfg(target_arch = "wasm32")]
        {
            let command = match self.gx_store.unified_sidebar.picker_target.take() {
                Some(group_id) => json!({"type": "createSessionInGroup", "groupId": group_id}),
                None => json!({"type": "createSession"}),
            };
            self.dispatch_native_sidebar_command(command, cx);
        }
    }

    /// A pick in the tray's project chip: start the same agent as a draft there, with the composer's
    /// text, open it, then close this draft.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (Q847): "Changing the project or machine moves the draft, the same way the tray's move works". A session row can't change project, so like the worktree move this starts the same agent as a new session and closes the empty draft (`/api/draftWorktree {finish, moved}`, which only closes a session still a draft); it happens on the pick rather than on send, because the new chat has to be the one whose tray offers that project's worktrees. It reaches any machine through the launch's machine-scoped group id. If the launch fails the draft stays open with its text.
    pub(crate) fn gx_store_move_draft_to_project(
        &mut self,
        draft: PendingDraftMove,
        agent_id: &str,
        repo_key: &str,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.gx_store.unified_sidebar;
        host.saved
            .last_machines
            .insert(repo_key.to_string(), machine_id.to_string());
        host.persist();
        host.pending_draft_move = Some(draft);
        let message = json!({
            "type": "runSidebarAgent",
            "agentId": agent_id,
            "groupId": group_id_of(machine_id, project_id),
        });
        self.stage_agent_launch_placeholder(&message, cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// The new chat is open: gxserver closes the draft it replaced.
    pub(in crate::app) fn gx_store_finish_draft_move(
        &mut self,
        draft: Option<PendingDraftMove>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = draft else {
            return;
        };
        let params = json!({
            "projectId": draft.from_project_id,
            "sessionId": draft.from_session_id,
            "finish": true,
            "moved": true,
        });
        if draft.from_machine_id == LOCAL_MACHINE_ID {
            cx.spawn(async move |_, _| {
                let _ = super::rpc::gx_rpc(None, "/api/draftWorktree", params).await;
            })
            .detach();
        } else {
            self.start_gpui_remote_sidebar_rpc(
                &draft.from_machine_id,
                "/api/draftWorktree",
                Some(params),
                std::time::Duration::from_secs(15),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            )
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    /// The stray session: activating a project with no sessions starts its default agent, so the
    /// New session path must only ever aim the launch.
    #[test]
    fn new_session_never_activates_a_project() {
        let source = include_str!("unified_new_session.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.contains(concat!("project_", "activation")));
        assert!(!code.contains(concat!("activate_", "project")));
    }
}

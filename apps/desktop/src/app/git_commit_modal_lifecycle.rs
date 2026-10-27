//! Open, command, and diff-delivery plumbing for the native commit review and file diff.
//! SEE-ALSO: apps/desktop/src/app/window/git_commit_modal/window.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/gx_store/git/review.rs and merge.rs (what each command does), apps/desktop/src/app/remote_conn/app_modal_bridge.rs (a `gitFileDiff` for an open review is delivered into it).
use crate::app::gx_store::git::GitReviewStagingAction;
use crate::app::gx_store::git::review::GitReviewConfirm;
use crate::app::gx_store::{read_preference_value, write_client_document_value};
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// The React host's `PROMPT_AGENT_MODAL_STORAGE_KEYS.gitCommit` (client-storage store `commitAgent`).
///
/// CDXC:AgentLauncher 2026-09-28 SEE-ALSO:
/// The React modal host still clears this key when Settings' Default Prompt Agent changes (`clearPromptAgentModalOverrides` in apps/desktop/views/modal-host.tsx (deleted 2026-10-01)); a native Settings has to keep doing that.
const PROMPT_AGENT_STORAGE_KEY: &str = "ghostex.promptAgent.gitCommit";
thread_local! {
    /// The Default Prompt Agent the open review resolved its agent with
    /// (`previousDefaultPromptAgentIdRef` of the React host).
    static OPEN_REVIEW_DEFAULT_PROMPT_AGENT: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// `hud.settings.defaultPromptAgentId` of a `sidebarState` message.
fn default_prompt_agent_of(sidebar_state: Option<&serde_json::Value>) -> Option<String> {
    sidebar_state
        .and_then(|state| state.get("hud"))
        .and_then(|hud| hud.get("settings"))
        .and_then(|settings| settings.get("defaultPromptAgentId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// `GIT_COMMIT_DIFF_PREFERENCES_STORAGE_KEY` (client-storage store `gitDiff`).
const DIFF_PREFERENCES_STORAGE_KEY: &str = "ghostex.gitCommitModal.diffPreferences.v1";

/// The commit review's List / Tree choice (client-storage store `gitChangesView`).
const CHANGES_VIEW_STORAGE_KEY: &str = "ghostex.gitCommitModal.changesView.v1";

/// The `hud.agents` rows with a launch command, the filter the React dialog applies.
fn git_commit_prompt_agents(hud: Option<&serde_json::Value>) -> Vec<GitCommitAgent> {
    hud.and_then(|hud| hud.get("agents"))
        .and_then(serde_json::Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter_map(|agent| {
                    let field = |key: &str| {
                        agent
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_string)
                    };
                    field("command")?;
                    Some(GitCommitAgent {
                        agent_id: field("agentId")?,
                        name: agent
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Writes one of the dialog's remembered values off the UI thread.
fn write_git_commit_preference(key: &'static str, raw: Option<String>, cx: &mut gpui::App) {
    cx.background_executor()
        .spawn(async move {
            let _ = write_client_document_value(key, raw.as_deref());
        })
        .detach();
}

fn review_confirm(confirm: GitCommitConfirm) -> GitReviewConfirm {
    GitReviewConfirm {
        request_id: confirm.request_id,
        message: confirm.message,
        agent_id: confirm.agent_id,
        file_paths: None,
        commit_on_new_ref: confirm.commit_on_new_ref,
        delete_worktree_after: confirm.delete_worktree_after,
    }
}

impl GhostexGpuiApp {
    /// Opens the native commit review for the `open` message of the `gitCommit` modal kind:
    /// `gitCommitDraft` (required) and the agent list and default prompt agent from
    /// `latestSidebarStateMessage.hud`.
    pub(crate) fn open_gpui_git_commit_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = message
            .get("gitCommitDraft")
            .and_then(GitCommitDraft::from_json)
        else {
            return;
        };
        let hud = message
            .get("latestSidebarStateMessage")
            .and_then(|state| state.get("hud"));
        let agents = git_commit_prompt_agents(hud);
        let default_agent_id = default_prompt_agent_of(message.get("latestSidebarStateMessage"));
        OPEN_REVIEW_DEFAULT_PROMPT_AGENT.set(default_agent_id.clone());
        let saved_agent_id = read_preference_value(PROMPT_AGENT_STORAGE_KEY)
            .ok()
            .flatten()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        // `resolvePromptAgentModalSelection`: the remembered choice, else the default, else the first.
        let has = |id: &Option<String>| {
            id.as_ref()
                .is_some_and(|id| agents.iter().any(|agent| &agent.agent_id == id))
        };
        let prompt_agent_id = if has(&saved_agent_id) {
            saved_agent_id
        } else if has(&default_agent_id) {
            default_agent_id
        } else {
            agents.first().map(|agent| agent.agent_id.clone())
        };
        let diff_prefs = GitDiffPrefs::from_storage(
            read_preference_value(DIFF_PREFERENCES_STORAGE_KEY)
                .ok()
                .flatten()
                .as_deref(),
        );
        let changes_view = GitChangesView::from_storage(
            read_preference_value(CHANGES_VIEW_STORAGE_KEY)
                .ok()
                .flatten()
                .as_deref(),
        );
        let config = GitCommitModalConfig {
            draft,
            agents,
            prompt_agent_id,
            diff_prefs,
            changes_view,
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_git_commit_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::GitCommit,
            GIT_COMMIT_MODAL_WIDTH,
            GIT_COMMIT_MODAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiGitCommitModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// The React host's `defaultPromptAgentId` effect for an open review: when a settings save
    /// changes the Default Prompt Agent, the review's agent falls back to the new default.
    pub(crate) fn reset_open_git_commit_prompt_agent(
        &mut self,
        sidebar_state: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let next = default_prompt_agent_of(Some(sidebar_state));
        let changed = OPEN_REVIEW_DEFAULT_PROMPT_AGENT.with_borrow_mut(|previous| {
            let changed = previous.is_some() && *previous != next;
            *previous = next.clone();
            changed
        });
        if changed {
            self.update_native_app_modal(
                GpuiAppModalKind::GitCommit,
                cx,
                |modal: &mut GpuiGitCommitModalWindow, _window, cx| {
                    modal.reset_prompt_agent(next.as_deref(), cx);
                },
            );
        }
    }

    /// Runs what the React host's `postMessage` reached through the modal bridge, then releases
    /// the window a closing command already removed.
    fn handle_gpui_git_commit_modal_command(
        &mut self,
        command: GitCommitModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::GitCommit;
        match command {
            GitCommitModalCommand::Confirm(confirm) => {
                self.release_native_app_modal_window(kind, cx);
                self.git_confirm_review(review_confirm(confirm), cx);
            }
            GitCommitModalCommand::DirectMerge(confirm) => {
                self.release_native_app_modal_window(kind, cx);
                self.git_confirm_direct_merge(review_confirm(confirm), cx);
            }
            GitCommitModalCommand::MultipleCommits {
                request_id,
                agent_id,
            } => {
                self.release_native_app_modal_window(kind, cx);
                self.git_run_multiple_commits(&request_id, agent_id, cx);
            }
            GitCommitModalCommand::Cancel { request_id } => {
                self.release_native_app_modal_window(kind, cx);
                self.git_cancel_review(&request_id, cx);
            }
            GitCommitModalCommand::OpenFileDiff {
                request_id,
                file_path,
                staged,
            } => self.git_open_changed_file_diff(
                &request_id,
                &file_path,
                Some(match staged {
                    true => ghostex_gx_core::git_menu::GitReviewSide::Staged,
                    false => ghostex_gx_core::git_menu::GitReviewSide::Unstaged,
                }),
                cx,
            ),
            GitCommitModalCommand::Staging {
                request_id,
                action,
                file_paths,
            } => self.git_review_staging(
                &request_id,
                match action {
                    GitCommitStagingAction::Stage => GitReviewStagingAction::Stage,
                    GitCommitStagingAction::Unstage => GitReviewStagingAction::Unstage,
                    GitCommitStagingAction::Discard => GitReviewStagingAction::Discard,
                },
                file_paths,
                cx,
            ),
            GitCommitModalCommand::ChangesViewChanged(view) => {
                write_git_commit_preference(
                    CHANGES_VIEW_STORAGE_KEY,
                    Some(view.to_storage().to_string()),
                    cx,
                );
            }
            GitCommitModalCommand::OpenFileLocation {
                request_id,
                file_path,
            } => self.git_open_changed_file(Some(&request_id), &file_path, true, cx),
            GitCommitModalCommand::PromptAgentChanged(agent_id) => {
                let agent_id = agent_id.trim().to_string();
                write_git_commit_preference(
                    PROMPT_AGENT_STORAGE_KEY,
                    (!agent_id.is_empty()).then_some(agent_id),
                    cx,
                );
            }
            GitCommitModalCommand::DiffPrefsChanged(prefs) => {
                write_git_commit_preference(
                    DIFF_PREFERENCES_STORAGE_KEY,
                    Some(prefs.to_storage()),
                    cx,
                );
            }
            GitCommitModalCommand::PathCopied => gpui_copy_feedback(cx),
        }
    }

    /// The review's file list after a Stage, Unstage or Discard (`changedFiles`, `requestId`), for
    /// the open commit review of that request.
    pub(crate) fn deliver_git_review_files(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = message
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let files = changed_files_from_json(message.get("changedFiles"));
        self.update_native_app_modal(
            GpuiAppModalKind::GitCommit,
            cx,
            |modal: &mut GpuiGitCommitModalWindow, _window, cx| {
                if modal.request_id() == request_id {
                    modal.receive_changes(files, cx);
                }
            },
        );
    }

    /// Delivers a `gitFileDiff` open message into the open commit review as its right pane's
    /// patch (`fileDiffDraft`). Returns false for any other message.
    pub(crate) fn receive_gpui_git_commit_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if message.get("modal").and_then(serde_json::Value::as_str) != Some("gitFileDiff") {
            return false;
        }
        let Some(draft) = message
            .get("gitFileDiff")
            .and_then(GitFileDiffDraft::from_json)
        else {
            return true;
        };
        self.update_native_app_modal(
            GpuiAppModalKind::GitCommit,
            cx,
            |modal: &mut GpuiGitCommitModalWindow, _window, cx| modal.receive_file_diff(draft, cx),
        );
        true
    }

    /// Opens the standalone File diff dialog for a `gitFileDiff` that arrived while no commit
    /// review is open; a newer one for an open File diff dialog replaces what it shows.
    pub(crate) fn open_gpui_git_file_diff_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = message
            .get("gitFileDiff")
            .and_then(GitFileDiffDraft::from_json)
        else {
            return;
        };
        let kind = GpuiAppModalKind::GitFileDiff;
        if self.native_app_modal_kind() == Some(kind) {
            let draft = draft.clone();
            if self
                .update_native_app_modal(
                    kind,
                    cx,
                    |modal: &mut GpuiGitFileDiffModalWindow, _window, cx| {
                        modal.set_draft(draft, cx)
                    },
                )
                .is_some()
            {
                return;
            }
        }
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            GitFileDiffModalCommand::Close => app.release_native_app_modal_window(kind, cx),
        });
        self.open_native_app_modal(
            kind,
            GIT_FILE_DIFF_MODAL_WIDTH,
            GIT_FILE_DIFF_MODAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiGitFileDiffModalWindow::new(draft, palette, host, window, cx))
            },
            cx,
        );
    }
}

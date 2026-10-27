//! The commit review window's state and behaviour (`GitCommitModal` in
//! packages/core-ui/git-commit-modal.tsx (deleted 2026-10-01)); drawing is in `render.rs`.
//!
//! CDXC:Git 2026-10-26 DECISION:
//! User: give the commit review "a staged/unstaged workflow and discard, like MonoCode's Source Control Changes panel" instead of committing everything with a Select mode to narrow it. The file rail has a Staged Changes and a Changes section with counts, per-file Stage (+) / Unstage (−) / Discard (↺) on hover, Stage All / Unstage All / Discard All on the section headers, a status letter per file, and a List / Tree toggle (List by default, remembered in `ghostex.gitCommitModal.changesView.v1`); a partly staged file is in both sections and each shows its own diff. Discard asks first, deletes an untracked file, restores a tracked one from the index, and Discard All never touches staged changes. Staging covers everything Select did, so Select and its checkboxes are gone (this supersedes the Select part of the 2026-09-28 decision below). Everything else from 2026-09-28 stays: the user asked to migrate every React modal to GPUI "looking and working exactly as now", so the dialog keeps its two-pane workspace (file rail with Show All, message editor, delete-after toggle; the diff pane with its three display controls), its footer agent select and actions, the Merge to main confirmation, the right-click Copy Path menu, and the same remembered agent (`ghostex.promptAgent.gitCommit`) and diff options (`ghostex.gitCommitModal.diffPreferences.v1`) in client storage.
//! SEE-ALSO: packages/core-ui/git-commit-modal.tsx (deleted 2026-10-01), git-file-diff-modal.tsx (deleted 2026-10-01), changed-files-tree.tsx (deleted 2026-10-01) and changed-files-tree-utils.ts (deleted 2026-10-01), and the `.git-commit-*`, `.git-file-diff-*` rules in packages/core-ui/styles/modals.css, modals-light.css and commands.css (the React twins); apps/desktop/src/app/git_commit_modal_lifecycle.rs (open, commands, diff delivery); apps/desktop/src/app/gx_store/git/review.rs (what the commands do); apps/desktop/src/bin/native_modal_demo/git_commit.rs (standalone preview).
use super::super::create_worktree_modal::trim_prompt_editor_trailing_spaces;
use super::super::native_modal_kit::*;
use super::diff::{GitDiffPrefs, GitDiffViewMode, all_files_diff_draft};
use super::diff_view::{GitDiffPalette, GitDiffSkin, GitDiffView};
use super::focus::{CommitFocus, FocusTarget, RowFocus};
use super::model::*;
use gpui::Focusable as _;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, KeyDownEvent, ListAlignment,
    ListState, Subscription, Window, px,
};
use gpui_component::input::{Escape, IndentInline, InputEvent, OutdentInline, TextareaState};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// `APP_MODAL_HOST_GIT_COMMIT_WINDOW_WIDTH` / `_HEIGHT`: the React dialog fills this child window.
pub(crate) const GIT_COMMIT_MODAL_WIDTH: f32 = 1078.0;
pub(crate) const GIT_COMMIT_MODAL_HEIGHT: f32 = 758.0;

/// A configured agent with a launch command (the React modal lists only those).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitCommitAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

/// `onConfirm` / `onDirectMerge`: the review this answers, the normalized message, the chosen
/// agent and the options. What it commits (the staged set, else every file) is the host's
/// decision over the review it holds (`review_commit_selection` in packages/gx-core).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitCommitConfirm {
    pub(crate) request_id: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) commit_on_new_ref: bool,
    pub(crate) delete_worktree_after: bool,
}

/// A file row's or section header's Stage, Unstage or Discard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitCommitStagingAction {
    Stage,
    Unstage,
    Discard,
}

/// The confirmation drawn over the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommitConfirmDialog {
    /// Merge to main.
    Merge,
    /// Discard the files named (`None`: every file under Changes).
    Discard {
        file_paths: Option<Vec<String>>,
        title: String,
        description: String,
        label: &'static str,
    },
}

/// A diff the right pane can show: one side of one file.
pub(crate) type DiffKey = (ChangeSide, String);

/// What the dialog asks its host to do. The closing commands (`Confirm`, `DirectMerge`,
/// `MultipleCommits`, `Cancel`) are sent after the dialog removed its own window.
pub(crate) enum GitCommitModalCommand {
    /// `confirmSidebarGitCommit`.
    Confirm(GitCommitConfirm),
    /// `confirmSidebarGitDirectMerge`.
    DirectMerge(GitCommitConfirm),
    /// `runSidebarGitMultipleCommits`.
    MultipleCommits {
        request_id: String,
        agent_id: Option<String>,
    },
    /// `cancelSidebarGitCommit`.
    Cancel { request_id: String },
    /// `openSidebarGitChangedFileDiff` for one side: the answer arrives through `receive_file_diff`.
    OpenFileDiff {
        request_id: String,
        file_path: String,
        staged: bool,
    },
    /// Stage, Unstage or Discard the files named (`None`: every file on that side). The new file
    /// list arrives through `receive_changes`.
    Staging {
        request_id: String,
        action: GitCommitStagingAction,
        file_paths: Option<Vec<String>>,
    },
    /// The List / Tree choice, remembered in `ghostex.gitCommitModal.changesView.v1`.
    ChangesViewChanged(GitChangesView),
    /// `openSidebarGitChangedFile` with `openLocation`.
    OpenFileLocation {
        request_id: String,
        file_path: String,
    },
    /// The footer agent choice, remembered in `ghostex.promptAgent.gitCommit`.
    PromptAgentChanged(String),
    /// The diff display options, remembered in `ghostex.gitCommitModal.diffPreferences.v1`.
    DiffPrefsChanged(GitDiffPrefs),
    /// Copy Path wrote the clipboard (the copy sound).
    PathCopied,
}

pub(crate) type GitCommitModalHost = Rc<dyn Fn(GitCommitModalCommand, &mut App)>;

pub(crate) struct GitCommitModalConfig {
    pub(crate) draft: GitCommitDraft,
    pub(crate) agents: Vec<GitCommitAgent>,
    /// The host's resolved prompt agent: the remembered choice, else Settings' default prompt
    /// agent, else the first agent (`resolvePromptAgentModalSelection`).
    pub(crate) prompt_agent_id: Option<String>,
    pub(crate) diff_prefs: GitDiffPrefs,
    pub(crate) changes_view: GitChangesView,
    pub(crate) palette: ModalPalette,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineDiffMode {
    All,
    File,
}

pub(crate) struct GpuiGitCommitModalWindow {
    pub(super) host: GitCommitModalHost,
    pub(super) palette: ModalPalette,
    pub(super) diff_palette: GitDiffPalette,
    pub(super) draft: GitCommitDraft,
    pub(super) agents: Vec<GitCommitAgent>,
    pub(super) selected_agent_id: String,
    /// Folded folders by `ReviewRow::key`, and folded sections.
    collapsed: HashSet<String>,
    collapsed_sections: HashSet<ChangeSide>,
    pub(super) view: GitChangesView,
    pub(super) rows: Rc<Vec<ReviewRow>>,
    pub(super) tree_list: ListState,
    /// The file row under the pointer, which shows its Stage / Unstage / Discard buttons.
    pub(super) hovered_row: Option<String>,
    /// A Stage, Unstage or Discard is running; the buttons wait for the new file list.
    pub(super) busy: bool,
    pub(super) delete_worktree_after: bool,
    pub(super) confirm_dialog: Option<CommitConfirmDialog>,
    /// The confirmation's focus trap and its two buttons; Radix focuses Cancel when it opens.
    pub(super) merge_trap_focus: FocusHandle,
    pub(super) merge_cancel_focus: FocusHandle,
    pub(super) merge_confirm_focus: FocusHandle,
    pub(super) focus: CommitFocus,
    pub(super) inline_mode: InlineDiffMode,
    diff_cache: HashMap<DiffKey, GitFileDiffDraft>,
    all_loading: Option<DiffKey>,
    pub(super) selected_diff: Option<DiffKey>,
    loading_diff: Option<DiffKey>,
    pub(super) prefs: GitDiffPrefs,
    pub(super) diff: GitDiffView,
    pub(super) message: Entity<TextareaState>,
    pub(super) agent_select: ModalSelect,
    pub(super) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiGitCommitModalWindow {
    pub(crate) fn new(
        config: GitCommitModalConfig,
        host: GitCommitModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = config.draft;
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Leave empty to auto-generate")
                .default_value(draft.initial_message())
        });
        let subscriptions = vec![cx.subscribe_in(
            &message,
            window,
            |_this: &mut Self, _input, event: &InputEvent, _window, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    cx.notify();
                }
            },
        )];
        let focus_handle = cx.focus_handle();
        // `autoFocus` on the message editor; without it the dialog frame takes the keys.
        if draft.show_commit_message {
            message.update(cx, |message, cx| message.focus(window, cx));
        } else {
            focus_handle.focus(window, cx);
        }
        let selected_agent_id = config
            .prompt_agent_id
            .filter(|id| config.agents.iter().any(|agent| &agent.agent_id == id))
            .or_else(|| config.agents.first().map(|agent| agent.agent_id.clone()))
            .unwrap_or_default();
        let view = config.changes_view;
        let rows = Rc::new(review_rows(
            &draft.changed_files,
            view,
            &HashSet::new(),
            &HashSet::new(),
        ));
        let tree_list = ListState::new(rows.len(), ListAlignment::Top, px(200.0));
        let initial_diff = all_entries(&draft.changed_files)
            .into_iter()
            .next()
            .map(|entry| (entry.side, entry.path));
        let mut this = Self {
            host,
            palette: config.palette,
            diff_palette: GitDiffPalette::resolve(&config.palette, GitDiffSkin::CommitReview),
            delete_worktree_after: draft.delete_worktree_after_default,
            draft,
            agents: config.agents,
            selected_agent_id,
            collapsed: HashSet::new(),
            collapsed_sections: HashSet::new(),
            view,
            rows,
            tree_list,
            hovered_row: None,
            busy: false,
            confirm_dialog: None,
            merge_trap_focus: cx.focus_handle(),
            merge_cancel_focus: cx.focus_handle().tab_stop(true),
            merge_confirm_focus: cx.focus_handle().tab_stop(true),
            focus: CommitFocus::new(cx),
            inline_mode: InlineDiffMode::File,
            diff_cache: HashMap::new(),
            all_loading: None,
            selected_diff: initial_diff.clone(),
            loading_diff: initial_diff.clone(),
            prefs: config.diff_prefs,
            diff: GitDiffView::new(window, cx),
            message,
            agent_select: ModalSelect::new(),
            focus_handle,
            _subscriptions: subscriptions,
        };
        this.diff.focus_on_select(this.focus_handle.clone(), cx);
        if let Some(key) = initial_diff {
            this.request_file_diff(key, cx);
        }
        this.sync_diff_view();
        this
    }

    pub(crate) fn request_id(&self) -> &str {
        &self.draft.request_id
    }

    fn request_file_diff(&self, (side, file_path): DiffKey, cx: &mut App) {
        (self.host)(
            GitCommitModalCommand::OpenFileDiff {
                request_id: self.draft.request_id.clone(),
                file_path,
                staged: side == ChangeSide::Staged,
            },
            cx,
        );
    }

    /// A `gitFileDiff` answer for the open review: cached, and drawn when it is the diff shown.
    pub(crate) fn receive_file_diff(&mut self, draft: GitFileDiffDraft, cx: &mut Context<Self>) {
        let side = match draft.staged {
            Some(true) => ChangeSide::Staged,
            _ => ChangeSide::Unstaged,
        };
        let key = (side, draft.file_path.clone());
        self.diff_cache.insert(key.clone(), draft);
        if self.loading_diff.as_ref() == Some(&key) {
            self.loading_diff = None;
        }
        if self.all_loading.as_ref() == Some(&key) {
            self.all_loading = None;
        }
        self.load_next_all_files_diff(cx);
        self.sync_diff_view();
        cx.notify();
    }

    /// The review's file list after a Stage, Unstage or Discard (`gitCommitChanges`): the sections
    /// are drawn again, every diff is read again (each side changed), and the diff shown stays on
    /// its file when that side still has it, else moves to the first row.
    pub(crate) fn receive_changes(&mut self, files: Vec<GitChangedFile>, cx: &mut Context<Self>) {
        self.draft.changed_files = files;
        self.busy = false;
        self.diff_cache.clear();
        self.all_loading = None;
        self.rebuild_rows();
        let entries = self.entries();
        let keep = self.selected_diff.as_ref().is_some_and(|(side, path)| {
            entries
                .iter()
                .any(|entry| entry.side == *side && &entry.path == path)
        });
        if !keep {
            self.selected_diff = entries
                .first()
                .map(|entry| (entry.side, entry.path.clone()));
        }
        self.loading_diff = None;
        match self.inline_mode {
            InlineDiffMode::All => self.load_next_all_files_diff(cx),
            InlineDiffMode::File => {
                if let Some(key) = self.selected_diff.clone() {
                    self.loading_diff = Some(key.clone());
                    self.request_file_diff(key, cx);
                }
            }
        }
        self.sync_diff_view();
        cx.notify();
    }

    /// Show All asks for the missing patches one at a time, in row order.
    fn load_next_all_files_diff(&mut self, cx: &mut App) {
        if self.inline_mode != InlineDiffMode::All || self.all_loading.is_some() {
            return;
        }
        let Some(key) = self
            .entries()
            .into_iter()
            .map(|entry| (entry.side, entry.path))
            .find(|key| !self.diff_cache.contains_key(key))
        else {
            return;
        };
        self.all_loading = Some(key.clone());
        self.request_file_diff(key, cx);
    }

    /// Every row's file, staged ones first.
    pub(super) fn entries(&self) -> Vec<ChangeEntry> {
        all_entries(&self.draft.changed_files)
    }

    fn rebuild_rows(&mut self) {
        self.rows = Rc::new(review_rows(
            &self.draft.changed_files,
            self.view,
            &self.collapsed,
            &self.collapsed_sections,
        ));
        self.tree_list.reset(self.rows.len());
    }

    pub(super) fn selected_diff_draft(&self) -> Option<&GitFileDiffDraft> {
        self.selected_diff
            .as_ref()
            .and_then(|key| self.diff_cache.get(key))
    }

    pub(super) fn is_selected_diff_loading(&self) -> bool {
        self.inline_mode == InlineDiffMode::File
            && self.selected_diff.is_some()
            && self.loading_diff == self.selected_diff
            && self.selected_diff_draft().is_none()
    }

    /// The patch the right pane draws: every row's for Show All, else the selected row's.
    fn active_patch(&self) -> Option<String> {
        match self.inline_mode {
            InlineDiffMode::All => {
                all_files_diff_draft(&self.entries(), &self.diff_cache).map(|draft| draft.patch)
            }
            InlineDiffMode::File => self.selected_diff_draft().map(|draft| draft.patch.clone()),
        }
    }

    pub(super) fn sync_diff_view(&mut self) {
        let patch = self.active_patch();
        self.diff.set_patch(patch.as_deref(), self.prefs);
    }

    /// `activeDiffStats`.
    pub(super) fn active_diff_stat(&self) -> GitFileStat {
        match self.inline_mode {
            InlineDiffMode::All => sum_stats(self.entries().iter().map(|entry| entry.stat)),
            InlineDiffMode::File => self
                .selected_diff_draft()
                .map(|draft| GitFileStat {
                    additions: draft.additions.unwrap_or(0),
                    deletions: draft.deletions.unwrap_or(0),
                })
                .unwrap_or_default(),
        }
    }

    /// The selected row's path; a staged diff says so, since the file may be under Changes too.
    pub(super) fn active_diff_label(&self) -> Option<String> {
        match self.inline_mode {
            InlineDiffMode::All => Some("All files".to_string()),
            InlineDiffMode::File => self.selected_diff.as_ref().map(|(side, path)| match side {
                ChangeSide::Staged => format!("{path} (staged)"),
                ChangeSide::Unstaged => path.clone(),
            }),
        }
    }

    pub(super) fn has_staged(&self) -> bool {
        self.draft
            .changed_files
            .iter()
            .any(|file| file.staged.is_some())
    }

    /// What Commit takes: the staged changes when there are any, else every change.
    pub(super) fn commit_stat(&self) -> GitFileStat {
        if self.has_staged() {
            sum_stats(
                self.draft
                    .changed_files
                    .iter()
                    .filter_map(|file| file.staged.as_ref().map(GitFileSide::stat)),
            )
        } else {
            summarize_changed_files(&self.draft.changed_files)
        }
    }

    /// `canConfirm`: a commit needs a changed file, and no Stage, Unstage or Discard running.
    pub(super) fn can_confirm(&self) -> bool {
        !self.busy && (!self.draft.show_commit_message || !self.draft.changed_files.is_empty())
    }

    pub(super) fn can_run_direct_merge(&self) -> bool {
        self.can_confirm() && !self.selected_agent_id.is_empty()
    }

    /// CDXC:Git 2026-05-28-07:47:
    /// Commit messages should not carry trailing spaces at line ends. Normalize pasted text in the textarea and normalize again before confirm, while a fully blank message still reaches native as empty so auto-generation works.
    fn trimmed_message(&self, cx: &App) -> String {
        trim_prompt_editor_trailing_spaces(&self.message.read(cx).value())
            .trim()
            .to_string()
    }

    fn agent_id(&self) -> Option<String> {
        (!self.selected_agent_id.is_empty()).then(|| self.selected_agent_id.clone())
    }

    fn confirm_payload(&self, commit_on_new_ref: bool, cx: &App) -> GitCommitConfirm {
        GitCommitConfirm {
            request_id: self.draft.request_id.clone(),
            message: self.trimmed_message(cx),
            agent_id: self.agent_id(),
            commit_on_new_ref,
            delete_worktree_after: self.delete_worktree_after,
        }
    }

    pub(super) fn confirm(
        &mut self,
        commit_on_new_ref: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_confirm() {
            return;
        }
        let payload = self.confirm_payload(commit_on_new_ref, cx);
        window.remove_window();
        (self.host)(GitCommitModalCommand::Confirm(payload), cx);
    }

    pub(super) fn multiple_commits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_confirm() {
            return;
        }
        let command = GitCommitModalCommand::MultipleCommits {
            request_id: self.draft.request_id.clone(),
            agent_id: self.agent_id(),
        };
        window.remove_window();
        (self.host)(command, cx);
    }

    pub(super) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request_id = self.draft.request_id.clone();
        window.remove_window();
        (self.host)(GitCommitModalCommand::Cancel { request_id }, cx);
    }

    pub(super) fn open_merge_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_run_direct_merge() {
            return;
        }
        self.open_confirm_dialog(CommitConfirmDialog::Merge, window, cx);
    }

    fn open_confirm_dialog(
        &mut self,
        dialog: CommitConfirmDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_popups();
        self.confirm_dialog = Some(dialog);
        self.merge_cancel_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_merge_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_dialog = None;
        self.refocus(window, cx);
        cx.notify();
    }

    /// The confirmation's confirm button: Merge to main, or the discard it asked about.
    pub(super) fn accept_confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm_dialog.take() {
            Some(CommitConfirmDialog::Merge) => self.confirm_direct_merge(window, cx),
            Some(CommitConfirmDialog::Discard { file_paths, .. }) => {
                self.refocus(window, cx);
                self.run_staging(GitCommitStagingAction::Discard, file_paths, cx);
            }
            None => {}
        }
    }

    pub(super) fn confirm_direct_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_run_direct_merge() {
            return;
        }
        self.confirm_dialog = None;
        let payload = self.confirm_payload(false, cx);
        window.remove_window();
        (self.host)(GitCommitModalCommand::DirectMerge(payload), cx);
    }

    /// Back to the message editor (or the frame) after a popup or the confirmation closes.
    fn refocus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft.show_commit_message {
            self.message
                .update(cx, |message, cx| message.focus(window, cx));
        } else {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(super) fn open_inline_file_diff(&mut self, key: DiffKey, cx: &mut Context<Self>) {
        self.inline_mode = InlineDiffMode::File;
        self.selected_diff = Some(key.clone());
        self.loading_diff = Some(key.clone());
        self.request_file_diff(key, cx);
        self.sync_diff_view();
        cx.notify();
    }

    pub(super) fn show_all_file_diffs(&mut self, cx: &mut Context<Self>) {
        self.inline_mode = InlineDiffMode::All;
        self.loading_diff = None;
        self.load_next_all_files_diff(cx);
        self.sync_diff_view();
        cx.notify();
    }

    /// Folds or unfolds a folder, by its `ReviewRow::key`.
    pub(super) fn toggle_directory(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_string());
        }
        self.rebuild_rows();
        cx.notify();
    }

    pub(super) fn toggle_section(&mut self, side: ChangeSide, cx: &mut Context<Self>) {
        if !self.collapsed_sections.remove(&side) {
            self.collapsed_sections.insert(side);
        }
        self.rebuild_rows();
        cx.notify();
    }

    pub(super) fn set_changes_view(&mut self, view: GitChangesView, cx: &mut Context<Self>) {
        if self.view == view {
            return;
        }
        self.view = view;
        (self.host)(GitCommitModalCommand::ChangesViewChanged(view), cx);
        self.rebuild_rows();
        cx.notify();
    }

    pub(super) fn set_hovered_row(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        if self.hovered_row != key {
            self.hovered_row = key;
            cx.notify();
        }
    }

    /// Asks the host to Stage, Unstage or Discard; the rail waits for the new list.
    pub(super) fn run_staging(
        &mut self,
        action: GitCommitStagingAction,
        file_paths: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        (self.host)(
            GitCommitModalCommand::Staging {
                request_id: self.draft.request_id.clone(),
                action,
                file_paths,
            },
            cx,
        );
        cx.notify();
    }

    /// Discard asks first (MonoCode's wording): deleting an untracked file says so, and Discard
    /// All counts the files under Changes.
    pub(super) fn request_discard(
        &mut self,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let entries = side_entries(&self.draft.changed_files, ChangeSide::Unstaged);
        let named: Vec<&ChangeEntry> = match &path {
            Some(path) => entries.iter().filter(|entry| &entry.path == path).collect(),
            None => entries.iter().collect(),
        };
        let Some(first) = named.first() else {
            return;
        };
        let name = |entry: &ChangeEntry| {
            entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&entry.path)
                .to_string()
        };
        let untracked = named.iter().filter(|entry| entry.status == "U").count();
        let (title, description, label) = if named.len() == 1 && untracked == 1 {
            (
                "Delete untracked file?".to_string(),
                format!(
                    "Delete untracked file {}? This cannot be undone.",
                    name(first)
                ),
                "Delete",
            )
        } else if named.len() == 1 {
            (
                "Discard changes?".to_string(),
                format!(
                    "Discard changes in {}? Its staged changes are kept. This cannot be undone.",
                    name(first)
                ),
                "Discard",
            )
        } else {
            let deleted = match untracked {
                0 => String::new(),
                1 => " 1 untracked file will be deleted.".to_string(),
                count => format!(" {count} untracked files will be deleted."),
            };
            (
                "Discard all changes?".to_string(),
                format!(
                    "Discard all unstaged changes in {} files? Staged changes are kept.{deleted} This cannot be undone.",
                    named.len()
                ),
                "Discard All",
            )
        };
        self.open_confirm_dialog(
            CommitConfirmDialog::Discard {
                file_paths: path.map(|path| vec![path]),
                title,
                description,
                label,
            },
            window,
            cx,
        );
    }

    pub(super) fn toggle_delete_worktree_after(&mut self, cx: &mut Context<Self>) {
        self.delete_worktree_after = !self.delete_worktree_after;
        cx.notify();
    }

    pub(super) fn set_diff_prefs(&mut self, prefs: GitDiffPrefs, cx: &mut Context<Self>) {
        self.prefs = prefs;
        (self.host)(GitCommitModalCommand::DiffPrefsChanged(prefs), cx);
        self.sync_diff_view();
        cx.notify();
    }

    /// `setGitCommitPromptAgentId(undefined)` after the Default Prompt Agent changed: the new
    /// default, else the first agent.
    pub(crate) fn reset_prompt_agent(
        &mut self,
        default_agent_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.selected_agent_id = self
            .agents
            .iter()
            .find(|agent| Some(agent.agent_id.as_str()) == default_agent_id)
            .or_else(|| self.agents.first())
            .map(|agent| agent.agent_id.clone())
            .unwrap_or_default();
        cx.notify();
    }

    pub(super) fn selected_agent_index(&self) -> Option<usize> {
        self.agents
            .iter()
            .position(|agent| agent.agent_id == self.selected_agent_id)
    }

    pub(super) fn toggle_agent_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_select.toggle(self.selected_agent_index());
        if self.agent_select.open {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn choose_agent(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_select.close();
        if let Some(agent) = self.agents.get(index) {
            self.selected_agent_id = agent.agent_id.clone();
            (self.host)(
                GitCommitModalCommand::PromptAgentChanged(agent.agent_id.clone()),
                cx,
            );
        }
        self.refocus(window, cx);
        cx.notify();
    }

    pub(super) fn dismiss_agent_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_select.close();
        self.refocus(window, cx);
        cx.notify();
    }

    fn close_popups(&mut self) {
        self.agent_select.close();
    }

    /// CDXC:Git 2026-06-08-09:41:
    /// Commit review file rows need a right-click Copy path action. Keep the menu local to file rows so directory expansion and normal left-click diff preview behavior stay unchanged.
    pub(super) fn copy_file_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        (self.host)(GitCommitModalCommand::PathCopied, cx);
    }

    pub(super) fn open_file_location(&mut self, path: String, cx: &mut Context<Self>) {
        (self.host)(
            GitCommitModalCommand::OpenFileLocation {
                request_id: self.draft.request_id.clone(),
                file_path: path,
            },
            cx,
        );
    }

    /// Plain-text paste into the message drops trailing spaces per line (`handleMessagePaste`);
    /// a paste that changes nothing is left to the editor. The message field's paste hook, so it
    /// gets the field's own clipboard read, made off the UI thread (CDXC:Clipboard in
    /// helpers/clipboard_read.rs).
    pub(super) fn paste_message(
        &mut self,
        item: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(text) = item.text() else {
            return false;
        };
        let trimmed = trim_prompt_editor_trailing_spaces(&text);
        if text.is_empty() || trimmed == text {
            return false;
        }
        self.message.update(cx, |message, cx| {
            message.replace(trimmed, window, cx);
        });
        cx.notify();
        true
    }

    /// A browser textarea hands Tab and Shift+Tab to focus navigation; gpui-component's editor
    /// would indent the message instead.
    pub(super) fn on_indent_action(
        &mut self,
        _: &IndentInline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.message.read(cx).focus_handle(cx).is_focused(window) {
            cx.stop_propagation();
            window.focus_next(cx);
        }
    }

    pub(super) fn on_outdent_action(
        &mut self,
        _: &OutdentInline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.message.read(cx).focus_handle(cx).is_focused(window) {
            cx.stop_propagation();
            window.focus_prev(cx);
        }
    }

    /// The editor turns Escape into its own action; the dialog's Escape runs here first.
    pub(super) fn on_escape_action(
        &mut self,
        _: &Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.escape(window, cx);
    }

    /// Escape closes the innermost open layer: the agent list, the confirmation, then the dialog.
    /// (An open file menu takes Escape itself.)
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent_select.open {
            self.dismiss_agent_select(window, cx);
        } else if self.confirm_dialog.is_some() {
            self.close_merge_confirm(window, cx);
        } else {
            self.cancel(window, cx);
        }
    }

    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.escape(window, cx);
            cx.stop_propagation();
            return;
        }
        if self.confirm_dialog.is_some() {
            // The focused button answers Enter and Space; Tab walks the pair inside the trap.
            if matches!(key, "enter" | "space") {
                if self.merge_confirm_focus.is_focused(window) {
                    self.accept_confirm_dialog(window, cx);
                } else {
                    self.close_merge_confirm(window, cx);
                }
                cx.stop_propagation();
            }
            return;
        }
        if self.agent_select.open {
            match self.agent_select.handle_key(key, self.agents.len()) {
                ModalSelectKey::Consumed => {
                    if !self.agent_select.open {
                        self.refocus(window, cx);
                    }
                    cx.notify();
                }
                ModalSelectKey::Choose(index) => self.choose_agent(index, window, cx),
                ModalSelectKey::Ignored => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.activate_focused(key, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Enter or Space on the focused control, the way a browser activates a focused button (both
    /// keys), a checkbox (Space only) or a select trigger (Enter, Space and the arrows open it).
    fn activate_focused(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let button = matches!(key, "enter" | "space");
        if let Some(row) = self.focus.focused_row(window) {
            match row {
                RowFocus::Section(side) if button => self.toggle_section(side, cx),
                RowFocus::Directory(key) if button => self.toggle_directory(&key, cx),
                RowFocus::Open(side, path) if button => {
                    self.open_inline_file_diff((side, path), cx)
                }
                _ => return false,
            }
            return true;
        }
        let Some(target) = self.focus.focused_target(window) else {
            return false;
        };
        let prefs = self.prefs;
        match target {
            FocusTarget::DeleteAfter if key == "space" => self.toggle_delete_worktree_after(cx),
            FocusTarget::Agent if matches!(key, "enter" | "space" | "up" | "down") => {
                self.toggle_agent_select(window, cx)
            }
            _ if !button => return false,
            FocusTarget::ShowAll => self.show_all_file_diffs(cx),
            FocusTarget::ViewMode => self.set_diff_prefs(
                GitDiffPrefs {
                    view_mode: match prefs.view_mode {
                        GitDiffViewMode::Split => GitDiffViewMode::Unified,
                        GitDiffViewMode::Unified => GitDiffViewMode::Split,
                    },
                    ..prefs
                },
                cx,
            ),
            // Disabled while side by side forces wrapping.
            FocusTarget::LineWrap if prefs.view_mode == GitDiffViewMode::Split => {}
            FocusTarget::LineWrap => self.set_diff_prefs(
                GitDiffPrefs {
                    line_wrap: !prefs.line_wrap,
                    ..prefs
                },
                cx,
            ),
            FocusTarget::Whitespace => self.set_diff_prefs(
                GitDiffPrefs {
                    hide_whitespace: !prefs.hide_whitespace,
                    ..prefs
                },
                cx,
            ),
            FocusTarget::Cancel => self.cancel(window, cx),
            FocusTarget::Merge => self.open_merge_confirm(window, cx),
            FocusTarget::NewBranch => self.confirm(true, window, cx),
            FocusTarget::Multiple => self.multiple_commits(window, cx),
            FocusTarget::Confirm => self.confirm(false, window, cx),
            FocusTarget::DeleteAfter | FocusTarget::Agent => {
                return false;
            }
        }
        true
    }

    /// Preview hooks for the standalone demo binary.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/git_commit.rs only
    pub(crate) fn preview_state(
        &mut self,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match state {
            "tree" => self.set_changes_view(GitChangesView::Tree, cx),
            "hover" => {
                self.hovered_row = self
                    .rows
                    .iter()
                    .find(|row| matches!(row.kind, ReviewRowKind::File { .. }))
                    .map(ReviewRow::key);
            }
            "discard" => self.request_discard(None, window, cx),
            "all" => self.show_all_file_diffs(cx),
            "confirm" => self.open_merge_confirm(window, cx),
            "agents" => self.toggle_agent_select(window, cx),
            "split" => {
                let prefs = GitDiffPrefs {
                    view_mode: GitDiffViewMode::Split,
                    ..self.prefs
                };
                self.set_diff_prefs(prefs, cx);
            }
            "wrap" => {
                let prefs = GitDiffPrefs {
                    line_wrap: true,
                    ..self.prefs
                };
                self.set_diff_prefs(prefs, cx);
            }
            "collapsed" => {
                self.set_changes_view(GitChangesView::Tree, cx);
                if let Some(key) = self
                    .rows
                    .iter()
                    .find(|row| matches!(row.kind, ReviewRowKind::Directory { .. }))
                    .map(ReviewRow::key)
                {
                    self.toggle_directory(&key, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }
}

impl ModalCornerClose for GpuiGitCommitModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}

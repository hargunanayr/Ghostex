//! Stage, Unstage and Discard from the commit review dialog: each request is checked against the
//! review's own file list, run by gxserver (`addAll`, `unstageFiles`, `discardFiles` in
//! server/src/typed_operations/git.rs), and answered with the project's Git state read again, which
//! becomes the review's file list and the dialog's two sections.
//!
//! CDXC:Git 2026-10-26 WHY:
//! The review trusts only the paths it listed (CDXC:Git 2026-06-24-15:22 on `PendingGitReview`), so
//! a staging request names listed paths on the side it acts on, and Stage All, Unstage All and
//! Discard All send the listed paths rather than an unbounded `git add -A`, `git reset` or
//! `git clean`. After each one the list is replaced by a fresh read, since staging changes what is
//! on each side and the dialog shows (and so the user sees) the new list before any commit.

use ghostex_gx_core::git_menu::{
    GitChangedFile, GitReviewSide, GitToastLevel, review_side_all_paths, review_side_paths,
};
use serde_json::{Value, json};

use super::calls;
use super::hud::GitReadOptions;
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;

/// `FILE_PATH_LIMIT` of gxserver's typed operations: one call names at most this many paths.
const PATHS_PER_CALL: usize = 500;

/// What a review row's or section header's button asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitReviewStagingAction {
    Stage,
    Unstage,
    Discard,
}

impl GitReviewStagingAction {
    fn side(self) -> GitReviewSide {
        match self {
            Self::Stage | Self::Discard => GitReviewSide::Unstaged,
            Self::Unstage => GitReviewSide::Staged,
        }
    }

    fn git_action(self) -> &'static str {
        match self {
            Self::Stage => "addAll",
            Self::Unstage => "unstageFiles",
            Self::Discard => "discardFiles",
        }
    }

    fn failure_title(self) -> &'static str {
        match self {
            Self::Stage => "Could not stage changes",
            Self::Unstage => "Could not unstage changes",
            Self::Discard => "Could not discard changes",
        }
    }
}

fn failure_text(result: &Value) -> String {
    let stderr = result.get("stderr").and_then(Value::as_str).unwrap_or("");
    match stderr.trim() {
        "" => "Git did not finish the request.".to_string(),
        text => text.lines().next().unwrap_or(text).to_string(),
    }
}

impl GhostexGpuiApp {
    /// A Stage, Unstage or Discard from the review `request_id`: `file_paths` are the rows it names,
    /// `None` every file on the side it acts on (Stage All, Unstage All, Discard All).
    pub(crate) fn git_review_staging(
        &mut self,
        request_id: &str,
        action: GitReviewStagingAction,
        file_paths: Option<Vec<String>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(review) = self.gx_store.git.reviews.get(request_id).cloned() else {
            return;
        };
        let paths = match &file_paths {
            Some(paths) => review_side_paths(&review, action.side(), paths),
            None => Ok(review_side_all_paths(&review, action.side())),
        };
        let scope = self.git_scope_for_project(&review.project_id);
        let (paths, scope) = match (paths, scope) {
            (Ok(paths), Ok(scope)) if !paths.is_empty() => (paths, scope),
            (paths, scope) => {
                let description = match (paths, scope) {
                    (Err(reason), _) => reason.to_string(),
                    (_, Err(_)) => "The review's project is no longer available.".to_string(),
                    _ => "There is nothing to change on that side of the review.".to_string(),
                };
                self.git_toast(
                    GitToastLevel::Warning,
                    action.failure_title(),
                    ToastOptions::described(description),
                    cx,
                );
                return self.git_review_files_changed(request_id, &review.files, cx);
            }
        };
        let remote = scope.remote.clone();
        let project_id = scope.project_id();
        let request_id = request_id.to_string();
        cx.spawn(async move |this, cx| {
            let mut failure = None;
            for chunk in paths.chunks(PATHS_PER_CALL) {
                let answer = calls::git_action(
                    remote.clone(),
                    project_id.clone(),
                    action.git_action(),
                    json!({ "filePaths": chunk }),
                )
                .await;
                match answer {
                    Ok(result) if calls::exit_code(&result) == 0 => {}
                    Ok(result) => failure = Some(failure_text(&result)),
                    Err(error) => failure = Some(error.message),
                }
                if failure.is_some() {
                    break;
                }
            }
            let Ok(read) = this.update(cx, |this, cx| {
                if let Some(description) = failure {
                    this.git_toast(
                        GitToastLevel::Error,
                        action.failure_title(),
                        ToastOptions::described(description),
                        cx,
                    );
                }
                this.git_refresh_state(
                    scope,
                    GitReadOptions {
                        force: true,
                        ..GitReadOptions::default()
                    },
                    cx,
                )
            }) else {
                return;
            };
            let state = read.await;
            let _ = this.update(cx, |this, cx| {
                let Some(review) = this.gx_store.git.reviews.get_mut(&request_id) else {
                    return;
                };
                if state.is_repo {
                    review.files = state.files.clone();
                    review.has_commit = state.has_working_tree_changes;
                }
                let files = review.files.clone();
                this.git_review_files_changed(&request_id, &files, cx);
            });
        })
        .detach();
    }

    /// Hands the review's current file list to its open dialog (`gitCommitChanges`), which redraws
    /// its sections and asks again for the diff it shows.
    fn git_review_files_changed(
        &mut self,
        request_id: &str,
        files: &[GitChangedFile],
        cx: &mut gpui::Context<Self>,
    ) {
        self.deliver_git_review_files(
            json!({ "changedFiles": files, "requestId": request_id }),
            cx,
        );
    }
}

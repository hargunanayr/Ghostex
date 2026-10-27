//! Native GPUI Git commit review and its file diff, the desktop twins of the React
//! `GitCommitModal` (packages/core-ui/git-commit-modal.tsx (deleted 2026-10-01)) and `GitFileDiffModal`
//! (packages/core-ui/git-file-diff-modal.tsx (deleted 2026-10-01)). The decision record is on `window.rs`.
//!
//! This module depends only on gpui, gpui-component (with its gpui-base text selection) and
//! `native_modal_kit/` (plus the shared prompt-text trim in `create_worktree_modal.rs`) so the
//! preview binary can include it.
mod diff;
mod diff_select;
mod diff_view;
mod diff_wrap;
mod file_diff_window;
mod focus;
mod model;
mod render;
mod window;

pub(crate) use diff::GitDiffPrefs;
pub(crate) use file_diff_window::*;
pub(crate) use model::{GitChangesView, GitCommitDraft, GitFileDiffDraft};
pub(crate) use window::{
    GIT_COMMIT_MODAL_HEIGHT, GIT_COMMIT_MODAL_WIDTH, GitCommitAgent, GitCommitConfirm,
    GitCommitModalCommand, GitCommitModalConfig, GpuiGitCommitModalWindow,
};
// Named only by the app's host (git_commit_modal_lifecycle.rs), not by the preview binary.
#[allow(unused_imports)]
pub(crate) use model::changed_files_from_json;
#[allow(unused_imports)]
pub(crate) use window::GitCommitStagingAction;
// Named only by the preview binary (src/bin/native_modal_demo/git_commit.rs).
#[allow(unused_imports)]
pub(crate) use diff::GitDiffViewMode;
#[allow(unused_imports)]
pub(crate) use model::{GitChangedFile, GitFileSide};
#[allow(unused_imports)]
pub(crate) use window::GitCommitModalHost;

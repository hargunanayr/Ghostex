//! The commit review dialog: the request a review is opened with, the draft the dialog draws, and
//! the check that a confirmed selection only names files that review listed.

use serde_json::{json, Value};

use super::state::{GitChangedFile, GitReviewAction, GitState};
use super::text::{confirm_label, prompt_description};

/// A remote project a review belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRemoteProject {
    pub machine_id: String,
    pub project_id: String,
}

/// `GpuiPendingGitCommitRequest`: what a review request id stands for until the dialog confirms,
/// cancels or starts Multiple Commits.
///
/// CDXC:Git 2026-06-24-15:22:
/// GPUI commit review stores the gxserver-derived changed-file list with the request id. Later
/// modal selections and diff clicks may only reference those paths, so CEF cannot stage or inspect
/// arbitrary renderer-supplied paths. Treat the modal's all-selected case as that stored review
/// list instead of a fresh unbounded add-all, so files created after review opens cannot slip into
/// the confirmed commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingGitReview {
    pub action: GitReviewAction,
    pub files: Vec<GitChangedFile>,
    pub has_commit: bool,
    /// The project on this computer, or the remote project on its machine.
    pub project_id: String,
    pub remote: Option<GitRemoteProject>,
}

impl PendingGitReview {
    pub fn has_file(&self, path: &str) -> bool {
        self.files.iter().any(|file| file.path == path)
    }

    pub fn file(&self, path: &str) -> Option<&GitChangedFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// `GpuiTrustedGitReviewFileSelection`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedFileSelection {
    /// The dialog named its files (rather than leaving every file selected).
    pub explicit: bool,
    pub file_paths: Vec<String>,
}

/// `resolveTrustedGitReviewFileSelection`: the files a confirm may commit. Every path must be one
/// the review listed; no path at all is an error.
pub fn resolve_trusted_file_selection(
    review: &PendingGitReview,
    file_paths: Option<&[String]>,
) -> Result<TrustedFileSelection, &'static str> {
    let explicit = file_paths.is_some();
    let candidates: Vec<String> = match file_paths {
        Some(paths) => paths.to_vec(),
        None => review.files.iter().map(|file| file.path.clone()).collect(),
    };
    let mut selected: Vec<String> = Vec::new();
    for candidate in candidates {
        let trusted = normalize_relative_git_file_path(&candidate)
            .filter(|path| review.has_file(path))
            .ok_or("Selected file is not part of the current Git review.")?;
        if !selected.contains(&trusted) {
            selected.push(trusted);
        }
    }
    if selected.is_empty() {
        return Err("Select at least one changed file.");
    }
    Ok(TrustedFileSelection {
        explicit,
        file_paths: selected,
    })
}

/// A side of the commit review's file list: Staged Changes or Changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitReviewSide {
    Staged,
    Unstaged,
}

/// What a confirmed review commits, and whether that is the index as it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewCommitSelection {
    pub selection: TrustedFileSelection,
    pub staged_only: bool,
}

/// The paths staged in a review, each staged rename followed by the path it came from (a rename is a deletion plus an addition in the index, and both go into the commit or come out of the index together).
pub fn staged_review_paths(files: &[GitChangedFile]) -> Vec<String> {
    let mut paths = Vec::new();
    for change in files.iter().filter_map(|file| {
        file.staged
            .as_ref()
            .map(|staged| (file.path.as_str(), staged))
    }) {
        let (path, staged) = change;
        for candidate in std::iter::once(path).chain(staged.original_path.as_deref()) {
            if !paths.iter().any(|known: &String| known == candidate) {
                paths.push(candidate.to_string());
            }
        }
    }
    paths
}

/// CDXC:Git 2026-10-26 DECISION:
/// User: "if anything is staged, Commit (and Commit on new branch / Multiple Commits, where that makes sense) commits only the staged set; if nothing is staged, keep today's commit-all behaviour (VS Code style smart commit)." The staged set comes from the review's own file list (refreshed after every Stage, Unstage or Discard the dialog runs), never from the dialog, and gxserver refuses the commit when the index holds anything that list did not show.
pub fn review_commit_selection(
    review: &PendingGitReview,
    file_paths: Option<&[String]>,
) -> Result<ReviewCommitSelection, &'static str> {
    let staged = staged_review_paths(&review.files);
    if !staged.is_empty() {
        return Ok(ReviewCommitSelection {
            selection: TrustedFileSelection {
                explicit: true,
                file_paths: staged,
            },
            staged_only: true,
        });
    }
    resolve_trusted_file_selection(review, file_paths).map(|selection| ReviewCommitSelection {
        selection,
        staged_only: false,
    })
}

/// The paths a Stage, Unstage or Discard from the review dialog may act on: each one listed on `side` of the review, and an unstaged rename's origin with it. No path at all is an error.
pub fn review_side_paths(
    review: &PendingGitReview,
    side: GitReviewSide,
    file_paths: &[String],
) -> Result<Vec<String>, &'static str> {
    let mut paths: Vec<String> = Vec::new();
    for candidate in file_paths {
        let file = normalize_relative_git_file_path(candidate)
            .and_then(|path| review.file(&path))
            .ok_or("The file is not part of the current Git review.")?;
        let change = match side {
            GitReviewSide::Staged => file.staged.as_ref(),
            GitReviewSide::Unstaged => file.unstaged.as_ref(),
        }
        .ok_or("The file has no changes on that side of the review.")?;
        for path in std::iter::once(file.path.as_str()).chain(change.original_path.as_deref()) {
            if !paths.iter().any(|known| known == path) {
                paths.push(path.to_string());
            }
        }
    }
    if paths.is_empty() {
        return Err("Choose at least one changed file.");
    }
    Ok(paths)
}

/// Every path on `side` of the review, for Stage All, Unstage All and Discard All.
pub fn review_side_all_paths(review: &PendingGitReview, side: GitReviewSide) -> Vec<String> {
    let listed: Vec<String> = review
        .files
        .iter()
        .filter(|file| match side {
            GitReviewSide::Staged => file.staged.is_some(),
            GitReviewSide::Unstaged => file.unstaged.is_some(),
        })
        .map(|file| file.path.clone())
        .collect();
    review_side_paths(review, side, &listed).unwrap_or_default()
}

/// `normalizeGpuiRelativeGitFilePath`: a project-relative path with forward slashes, or `None`
/// for anything that could leave the project.
pub fn normalize_relative_git_file_path(file_path: &str) -> Option<String> {
    let replaced = file_path.replace('\\', "/");
    let normalized = replaced.trim_start_matches('/').trim();
    if normalized.is_empty() || normalized.contains('\0') {
        return None;
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(normalized.to_string())
}

/// `parseGpuiSidebarGitCommitMessage`: the first line is the subject, the rest the body.
pub fn parse_commit_message(message: &str) -> (String, String) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return (String::new(), String::new());
    }
    let mut lines = trimmed
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let subject = lines.next().unwrap_or_default().trim().to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    (subject, body)
}

/// What the review dialog is opened with (`SidebarPromptGitCommitMessage`).
pub struct ReviewDraftInput<'a> {
    pub action: GitReviewAction,
    pub agent_id: Option<&'a str>,
    pub state: &'a GitState,
    pub is_worktree: bool,
    pub remote: bool,
    pub request_id: &'a str,
    pub worktree_name: Option<&'a str>,
}

/// The `gitCommitDraft` of the review dialog's open message.
pub fn review_modal_draft(input: ReviewDraftInput<'_>) -> Value {
    let has_commit = input.state.has_working_tree_changes;
    let description = if has_commit {
        if input.remote {
            "Review and confirm your remote commit. Leave the message blank to auto-generate one."
        } else {
            "Review and confirm your commit. Leave the message blank to auto-generate one."
        }
    } else {
        prompt_description(input.action)
    };
    let mut draft = json!({
        "action": input.action.selector(),
        "branch": input.state.branch,
        "changedFiles": input.state.files,
        "confirmLabel": confirm_label(input.action, has_commit),
        "deleteWorktreeAfterDefault": false,
        "description": description,
        "isDefaultRef": matches!(input.state.branch.as_deref(), Some("main") | Some("master")),
        "isWorktree": input.is_worktree,
        "requestId": input.request_id,
        "showCommitMessage": has_commit,
        "suggestedSubject": "",
        "type": "promptGitCommit",
    });
    if let Some(agent_id) = input.agent_id {
        draft["agentId"] = json!(agent_id);
    }
    if let Some(name) = input.worktree_name {
        draft["worktreeName"] = json!(name);
    }
    draft
}

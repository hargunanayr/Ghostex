//! The commit review's data: the review draft gxserver's Git menu opens it with, one file's
//! diff draft, and the rows of its Staged Changes and Changes sections, as a flat list or as the
//! changed-files tree (`packages/core-ui/changed-files-tree-utils.ts` (deleted 2026-10-01)).
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashSet;

/// A section of the file rail: Staged Changes (HEAD to the index) or Changes (the index to the
/// working tree, untracked files included).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChangeSide {
    Staged,
    Unstaged,
}

impl ChangeSide {
    pub(crate) const ALL: [ChangeSide; 2] = [ChangeSide::Staged, ChangeSide::Unstaged];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Staged => "Staged Changes",
            Self::Unstaged => "Changes",
        }
    }

    fn key(self, path: &str) -> String {
        match self {
            Self::Staged => format!("staged:{path}"),
            Self::Unstaged => format!("unstaged:{path}"),
        }
    }
}

/// One side of a changed file (`GitFileChange` in packages/gx-core/src/git_menu/state.rs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitFileSide {
    /// `M`, `A`, `D`, `R`, `U` (untracked) or `!` (a merge conflict).
    pub(crate) status: String,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
}

impl GitFileSide {
    pub(crate) fn stat(&self) -> GitFileStat {
        GitFileStat {
            additions: self.additions,
            deletions: self.deletions,
        }
    }
}

/// One `changedFiles[]` row (`SidebarGitChangedFile`), split into what is staged and what is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitChangedFile {
    pub(crate) path: String,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
    pub(crate) staged: Option<GitFileSide>,
    pub(crate) unstaged: Option<GitFileSide>,
}

impl GitChangedFile {
    pub(crate) fn side(&self, side: ChangeSide) -> Option<&GitFileSide> {
        match side {
            ChangeSide::Staged => self.staged.as_ref(),
            ChangeSide::Unstaged => self.unstaged.as_ref(),
        }
    }

    fn from_json(file: &Value) -> Option<Self> {
        let side = |key: &str| {
            let side = file.get(key).filter(|side| side.is_object())?;
            Some(GitFileSide {
                status: text(side, "status").unwrap_or_else(|| "M".to_string()),
                additions: count(side, "additions"),
                deletions: count(side, "deletions"),
            })
        };
        let additions = count(file, "additions");
        let deletions = count(file, "deletions");
        let staged = side("staged");
        // A gxserver from before the split (a remote computer not yet updated) names no side:
        // the whole change is listed under Changes.
        let unstaged = side("unstaged").or_else(|| {
            staged.is_none().then(|| GitFileSide {
                status: "M".to_string(),
                additions,
                deletions,
            })
        });
        Some(Self {
            path: text(file, "path")?,
            additions,
            deletions,
            staged,
            unstaged,
        })
    }
}

/// `changedFiles` of a review draft or of a `gitCommitChanges` refresh.
pub(crate) fn changed_files_from_json(files: Option<&Value>) -> Vec<GitChangedFile> {
    files
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(GitChangedFile::from_json).collect())
        .unwrap_or_default()
}

/// One file row of a section: the side it is on and that side's status and counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangeEntry {
    pub(crate) side: ChangeSide,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) stat: GitFileStat,
}

/// The files on `side`, in path order.
pub(crate) fn side_entries(files: &[GitChangedFile], side: ChangeSide) -> Vec<ChangeEntry> {
    let mut entries: Vec<ChangeEntry> = files
        .iter()
        .filter_map(|file| {
            file.side(side).map(|change| ChangeEntry {
                side,
                path: file.path.clone(),
                status: change.status.clone(),
                stat: change.stat(),
            })
        })
        .collect();
    entries.sort_by(|left, right| compare_by_name(&left.path, &right.path));
    entries
}

/// Every row's file in drawing order: Staged Changes, then Changes.
pub(crate) fn all_entries(files: &[GitChangedFile]) -> Vec<ChangeEntry> {
    ChangeSide::ALL
        .into_iter()
        .flat_map(|side| side_entries(files, side))
        .collect()
}

/// `ChangedFilesTreeStat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GitFileStat {
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
}

impl GitFileStat {
    pub(crate) fn is_zero(&self) -> bool {
        self.additions == 0 && self.deletions == 0
    }
}

/// `GitCommitModalDraft`, the `gitCommitDraft` of the `open` message
/// (`review_modal_draft` in packages/gx-core/src/git_menu/review.rs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitCommitDraft {
    pub(crate) request_id: String,
    /// `undefined` (no row), `null` (`(detached HEAD)`) or the branch name.
    pub(crate) branch: Option<Option<String>>,
    pub(crate) changed_files: Vec<GitChangedFile>,
    pub(crate) confirm_label: String,
    pub(crate) delete_worktree_after_default: bool,
    pub(crate) is_worktree: bool,
    pub(crate) show_commit_message: bool,
    pub(crate) suggested_subject: String,
    pub(crate) suggested_body: Option<String>,
    pub(crate) worktree_name: Option<String>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn count(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|n| n.max(0.0) as u64))
        })
        .unwrap_or(0)
}

impl GitCommitDraft {
    pub(crate) fn from_json(draft: &Value) -> Option<Self> {
        let request_id = text(draft, "requestId").filter(|id| !id.is_empty())?;
        let branch = match draft.get("branch") {
            None => None,
            Some(Value::String(branch)) => Some(Some(branch.clone())),
            Some(_) => Some(None),
        };
        let changed_files = changed_files_from_json(draft.get("changedFiles"));
        let flag = |key: &str| draft.get(key).and_then(Value::as_bool);
        Some(Self {
            request_id,
            branch,
            changed_files,
            confirm_label: text(draft, "confirmLabel").unwrap_or_else(|| "Commit".to_string()),
            delete_worktree_after_default: flag("deleteWorktreeAfterDefault") == Some(true),
            is_worktree: flag("isWorktree") == Some(true),
            show_commit_message: flag("showCommitMessage").unwrap_or(true),
            suggested_subject: text(draft, "suggestedSubject").unwrap_or_default(),
            suggested_body: text(draft, "suggestedBody"),
            worktree_name: text(draft, "worktreeName"),
        })
    }

    /// `buildDraftMessage`: the trimmed subject, a blank line and the trimmed body when there is one.
    pub(crate) fn initial_message(&self) -> String {
        let subject = self.suggested_subject.trim();
        match self.suggested_body.as_deref().map(str::trim) {
            Some(body) if !body.is_empty() => format!("{subject}\n\n{body}"),
            _ => subject.to_string(),
        }
    }
}

/// `SidebarGitFileDiffDraft`: one file's patch, delivered as the `gitFileDiff` open message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitFileDiffDraft {
    pub(crate) file_path: String,
    pub(crate) patch: String,
    pub(crate) additions: Option<u64>,
    pub(crate) deletions: Option<u64>,
    /// The side the patch shows; `None` for both (the standalone File diff dialog).
    pub(crate) staged: Option<bool>,
}

impl GitFileDiffDraft {
    pub(crate) fn from_json(draft: &Value) -> Option<Self> {
        let optional = |key: &str| {
            draft
                .get(key)
                .filter(|v| v.is_number())
                .map(|_| count(draft, key))
        };
        Some(Self {
            file_path: text(draft, "filePath")?,
            patch: text(draft, "patch").unwrap_or_default(),
            additions: optional("additions"),
            deletions: optional("deletions"),
            staged: draft.get("staged").and_then(Value::as_bool),
        })
    }
}

/// `summarizeChangedFiles`.
pub(crate) fn summarize_changed_files<'a>(
    files: impl IntoIterator<Item = &'a GitChangedFile>,
) -> GitFileStat {
    sum_stats(files.into_iter().map(|file| GitFileStat {
        additions: file.additions,
        deletions: file.deletions,
    }))
}

pub(crate) fn sum_stats(stats: impl IntoIterator<Item = GitFileStat>) -> GitFileStat {
    stats
        .into_iter()
        .fold(GitFileStat::default(), |total, stat| GitFileStat {
            additions: total.additions + stat.additions,
            deletions: total.deletions + stat.deletions,
        })
}

/// `ChangedFilesTreeNode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangedFilesTreeNode {
    Directory {
        name: String,
        path: String,
        stat: GitFileStat,
        children: Vec<ChangedFilesTreeNode>,
    },
    File {
        name: String,
        path: String,
        stat: GitFileStat,
    },
}

impl ChangedFilesTreeNode {
    fn name(&self) -> &str {
        match self {
            Self::Directory { name, .. } | Self::File { name, .. } => name,
        }
    }
}

#[derive(Default)]
struct MutableDirectory {
    /// Insertion order, like the JS `Map`.
    directories: Vec<(String, MutableDirectory)>,
    files: Vec<ChangedFilesTreeNode>,
    name: String,
    path: String,
    stat: GitFileStat,
}

/// The ICU root collation order of ASCII punctuation and symbols, which all sort
/// before digits and letters. Unlisted characters keep their code point order after these.
const PUNCTUATION_ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum CollationKey {
    Punctuation(usize),
    Other(u32),
    Digits(String),
    Letter(String),
}

fn collation_keys(text: &str) -> Vec<CollationKey> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            let mut digits = String::from(ch);
            while let Some(next) = chars.peek().copied().filter(char::is_ascii_digit) {
                digits.push(next);
                chars.next();
            }
            let trimmed = digits.trim_start_matches('0');
            // Numeric order: a longer run of significant digits is the larger number.
            keys.push(CollationKey::Digits(format!(
                "{:0>20}",
                if trimmed.is_empty() { "0" } else { trimmed }
            )));
        } else if ch.is_alphabetic() {
            keys.push(CollationKey::Letter(ch.to_lowercase().collect()));
        } else if let Some(rank) = PUNCTUATION_ORDER.find(ch) {
            keys.push(CollationKey::Punctuation(rank));
        } else {
            keys.push(CollationKey::Other(ch as u32));
        }
    }
    keys
}

/// `localeCompare(right, undefined, { numeric: true, sensitivity: 'base' })`: case-insensitive,
/// digit runs compared as numbers, punctuation before digits before letters.
pub(crate) fn compare_by_name(left: &str, right: &str) -> Ordering {
    collation_keys(left).cmp(&collation_keys(right))
}

fn compact_directory_node(node: ChangedFilesTreeNode) -> ChangedFilesTreeNode {
    let ChangedFilesTreeNode::Directory {
        name,
        path,
        stat,
        children,
    } = node
    else {
        return node;
    };
    let children: Vec<_> = children.into_iter().map(compact_directory_node).collect();
    let (mut name, mut path, mut stat, mut children) = (name, path, stat, children);
    while children.len() == 1 && matches!(children[0], ChangedFilesTreeNode::Directory { .. }) {
        let Some(ChangedFilesTreeNode::Directory {
            name: child_name,
            path: child_path,
            stat: child_stat,
            children: grandchildren,
        }) = children.pop()
        else {
            break;
        };
        name = format!("{name}/{child_name}");
        path = child_path;
        stat = child_stat;
        children = grandchildren;
    }
    ChangedFilesTreeNode::Directory {
        name,
        path,
        stat,
        children,
    }
}

fn to_tree_nodes(directory: MutableDirectory) -> Vec<ChangedFilesTreeNode> {
    let mut subdirectories = directory.directories;
    subdirectories.sort_by(|left, right| compare_by_name(&left.1.name, &right.1.name));
    let mut nodes: Vec<ChangedFilesTreeNode> = subdirectories
        .into_iter()
        .map(|(_, subdirectory)| {
            let name = subdirectory.name.clone();
            let path = subdirectory.path.clone();
            let stat = subdirectory.stat;
            compact_directory_node(ChangedFilesTreeNode::Directory {
                name,
                path,
                stat,
                children: to_tree_nodes(subdirectory),
            })
        })
        .collect();
    let mut files = directory.files;
    files.sort_by(|left, right| compare_by_name(left.name(), right.name()));
    nodes.extend(files);
    nodes
}

/// `buildChangedFilesTree`.
///
/// CDXC:Worktrees 2026-05-18-23:07:
/// Git review surfaces show changed files as a compact directory tree with aggregated additions/deletions so worktree commit/PR flows can select files without flattening large project paths.
pub(crate) fn build_changed_files_tree(files: &[GitChangedFile]) -> Vec<ChangedFilesTreeNode> {
    let mut root = MutableDirectory::default();
    for file in files {
        let segments: Vec<&str> = file
            .path
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty())
            .collect();
        let Some((file_name, parents)) = segments.split_last() else {
            continue;
        };
        let stat = GitFileStat {
            additions: file.additions,
            deletions: file.deletions,
        };
        root.stat.additions += stat.additions;
        root.stat.deletions += stat.deletions;
        let mut current = &mut root;
        for segment in parents {
            let next_path = if current.path.is_empty() {
                segment.to_string()
            } else {
                format!("{}/{segment}", current.path)
            };
            let index = match current
                .directories
                .iter()
                .position(|(name, _)| name == segment)
            {
                Some(index) => index,
                None => {
                    current.directories.push((
                        segment.to_string(),
                        MutableDirectory {
                            name: segment.to_string(),
                            path: next_path,
                            ..MutableDirectory::default()
                        },
                    ));
                    current.directories.len() - 1
                }
            };
            current = &mut current.directories[index].1;
            current.stat.additions += stat.additions;
            current.stat.deletions += stat.deletions;
        }
        current.files.push(ChangedFilesTreeNode::File {
            name: file_name.to_string(),
            path: segments.join("/"),
            stat,
        });
    }
    to_tree_nodes(root)
}

/// How a section lists its files; remembered in `ghostex.gitCommitModal.changesView.v1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GitChangesView {
    /// One row per file: its name, then its folder dimmed.
    #[default]
    List,
    /// The folder tree.
    Tree,
}

impl GitChangesView {
    pub(crate) fn from_storage(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("tree") => Self::Tree,
            _ => Self::List,
        }
    }

    pub(crate) fn to_storage(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Tree => "tree",
        }
    }
}

/// What a drawn row of the file rail is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewRowKind {
    /// A section header with its file count.
    Section {
        count: usize,
        expanded: bool,
    },
    Directory {
        expanded: bool,
    },
    /// `folder` is the dimmed folder a list row shows after the name.
    File {
        status: String,
        folder: Option<String>,
    },
}

/// One drawn row of the file rail, flattened for the virtual list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReviewRow {
    pub(crate) side: ChangeSide,
    pub(crate) depth: usize,
    pub(crate) name: String,
    /// The file or folder path; empty for a section header.
    pub(crate) path: String,
    pub(crate) stat: GitFileStat,
    pub(crate) kind: ReviewRowKind,
    /// The first row of a section after the first: a gap sits above it.
    pub(crate) section_gap: bool,
}

impl ReviewRow {
    /// The key a folder's collapse state and a row's focus handle are kept under.
    pub(crate) fn key(&self) -> String {
        self.side.key(&self.path)
    }
}

/// The rows the file rail draws: each section that has files (header, then its files as a list
/// or a tree), every folder expanded unless the user folded it.
pub(crate) fn review_rows(
    files: &[GitChangedFile],
    view: GitChangesView,
    collapsed: &HashSet<String>,
    collapsed_sections: &HashSet<ChangeSide>,
) -> Vec<ReviewRow> {
    fn walk(
        side: ChangeSide,
        nodes: &[ChangedFilesTreeNode],
        depth: usize,
        entries: &[ChangeEntry],
        collapsed: &HashSet<String>,
        rows: &mut Vec<ReviewRow>,
    ) {
        for node in nodes {
            match node {
                ChangedFilesTreeNode::Directory {
                    name,
                    path,
                    stat,
                    children,
                } => {
                    let expanded = !collapsed.contains(&side.key(path));
                    rows.push(ReviewRow {
                        side,
                        depth,
                        name: name.clone(),
                        path: path.clone(),
                        stat: *stat,
                        kind: ReviewRowKind::Directory { expanded },
                        section_gap: false,
                    });
                    if expanded {
                        walk(side, children, depth + 1, entries, collapsed, rows);
                    }
                }
                ChangedFilesTreeNode::File { name, path, stat } => rows.push(ReviewRow {
                    side,
                    depth,
                    name: name.clone(),
                    path: path.clone(),
                    stat: *stat,
                    kind: ReviewRowKind::File {
                        status: entries
                            .iter()
                            .find(|entry| &entry.path == path)
                            .map(|entry| entry.status.clone())
                            .unwrap_or_default(),
                        folder: None,
                    },
                    section_gap: false,
                }),
            }
        }
    }
    let mut rows = Vec::new();
    for side in ChangeSide::ALL {
        let entries = side_entries(files, side);
        if entries.is_empty() {
            continue;
        }
        let expanded = !collapsed_sections.contains(&side);
        rows.push(ReviewRow {
            side,
            depth: 0,
            name: side.title().to_string(),
            path: String::new(),
            stat: sum_stats(entries.iter().map(|entry| entry.stat)),
            kind: ReviewRowKind::Section {
                count: entries.len(),
                expanded,
            },
            section_gap: !rows.is_empty(),
        });
        if !expanded {
            continue;
        }
        match view {
            GitChangesView::List => {
                rows.extend(entries.iter().map(|entry| {
                    let (folder, name) = match entry.path.rsplit_once('/') {
                        Some((folder, name)) => (Some(folder.to_string()), name.to_string()),
                        None => (None, entry.path.clone()),
                    };
                    ReviewRow {
                        side,
                        depth: 0,
                        name,
                        path: entry.path.clone(),
                        stat: entry.stat,
                        kind: ReviewRowKind::File {
                            status: entry.status.clone(),
                            folder,
                        },
                        section_gap: false,
                    }
                }));
            }
            GitChangesView::Tree => {
                let side_files: Vec<GitChangedFile> = entries
                    .iter()
                    .map(|entry| GitChangedFile {
                        path: entry.path.clone(),
                        additions: entry.stat.additions,
                        deletions: entry.stat.deletions,
                        staged: None,
                        unstaged: None,
                    })
                    .collect();
                let tree = build_changed_files_tree(&side_files);
                walk(side, &tree, 0, &entries, collapsed, &mut rows);
            }
        }
    }
    rows
}

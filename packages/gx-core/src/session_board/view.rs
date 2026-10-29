//! The Board as data: four lanes of cards, the Space's project filter, and what each card offers.

use std::collections::BTreeMap;
use std::sync::Arc;

use ghostex_gx_protocol::LifecycleState;

use super::columns::{board_column, bucket_facts, BoardColumn};
use super::scope::{board_scope, BoardSpace};
use crate::core::Core;
use crate::keys::SessionKey;
use crate::sidebar_view::{SessionRow, SidebarInputs, WorktreeView};

/// All of the Space's projects, or one of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoardFilter {
    #[default]
    All,
    Project(String),
}

/// The one status a card spells out next to its project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardCardStatus {
    #[default]
    None,
    Working,
    /// Start was pressed and the prompt has not reached the agent yet.
    Starting,
    /// A question or a permission prompt waits on the user (the sidebar's pink dot).
    NeedsAnswer,
    /// A finished turn the user has not seen (the blue dot).
    Unseen,
    Sleeping,
    Error,
}

impl BoardCardStatus {
    pub fn label(self) -> Option<&'static str> {
        match self {
            BoardCardStatus::None => None,
            BoardCardStatus::Working => Some("Working"),
            BoardCardStatus::Starting => Some("Starting…"),
            BoardCardStatus::NeedsAnswer => Some("Needs your answer"),
            BoardCardStatus::Unseen => Some("Done"),
            BoardCardStatus::Sleeping => Some("Sleeping"),
            BoardCardStatus::Error => Some("Stopped with an error"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCard {
    pub key: SessionKey,
    pub sidebar_session_id: String,
    pub column: BoardColumn,
    pub status: BoardCardStatus,
    pub project_id: String,
    pub project_title: String,
    /// The collection the project sits in, by title.
    pub project_group: Option<String>,
    /// The orchestrator this thread reports to, by title.
    pub thread_of: Option<String>,
    /// A Board task's prompt, cut for the card.
    pub task_preview: Option<String>,
    /// A Board task still waiting for Start: the card offers Start and a click opens its editor,
    /// because focusing a draft would start its agent.
    pub task_waiting: bool,
    pub completed_at: Option<String>,
    /// The git branch the agent works on, when the daemon knows it.
    pub branch: Option<String>,
    /// The worktree the agent works in, by name, when its project is a worktree.
    pub worktree: Option<String>,
    /// The sidebar's row: title, agent icon and the times the card draws.
    pub row: Arc<SessionRow>,
}

impl BoardCard {
    pub fn title(&self) -> &str {
        &self.row.display_title
    }

    /// The project as the card names it, after its collection: "XMOR - forever".
    pub fn project_label(&self) -> String {
        project_label(self.project_group.as_deref(), &self.project_title)
    }

    pub fn can_complete(&self) -> bool {
        self.column == BoardColumn::Review
    }

    pub fn can_reopen(&self) -> bool {
        self.column == BoardColumn::Completed
    }

    /// Complete all leaves a thread that waits on the user's answer in Review.
    pub fn waits_on_answer(&self) -> bool {
        self.status == BoardCardStatus::NeedsAnswer
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardLane {
    pub column: BoardColumn,
    pub cards: Vec<BoardCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardProject {
    pub project_id: String,
    pub title: String,
    /// The collection the project sits in, by title.
    pub group: Option<String>,
    /// The project's folder, which the project pickers show and search.
    pub path: Option<String>,
    /// The project's cards in every column.
    pub count: usize,
}

/// A project named after its collection, the way the cards and pickers show it.
///
/// CDXC:SessionBoard 2026-10-28 DECISION:
/// User (Ryan): "Di samping nama project, kasih nama groupnya (Misal XMOR - forever)". The group is the collection (the sidebar's coloured folder) the project sits in; a project outside any collection keeps its bare name.
fn project_label(group: Option<&str>, title: &str) -> String {
    match group {
        Some(group) => format!("{group} - {title}"),
        None => title.to_string(),
    }
}

impl BoardProject {
    /// The project as the pickers name it, after its collection.
    pub fn label(&self) -> String {
        project_label(self.group.as_deref(), &self.title)
    }

    /// Whether the project pickers' search `query` finds this project: by its title, its
    /// collection or its folder, ignoring case. An empty query finds every project.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.label().to_lowercase().contains(&query)
            || self
                .path
                .as_deref()
                .is_some_and(|path| path.to_lowercase().contains(&query))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionBoardView {
    /// The Space the Board covers; `None` when Spaces are off.
    pub space_name: Option<String>,
    /// The Spaces the Board's Space switcher offers; empty when Spaces are off.
    pub spaces: Vec<BoardSpace>,
    pub projects: Vec<BoardProject>,
    /// The filter in force: a project that left the Space reads as All.
    pub filter: BoardFilter,
    /// Pending, In progress, Review and Completed, in that order.
    pub lanes: Vec<BoardLane>,
    /// Cards across the lanes, after the filter.
    pub total: usize,
}

impl SessionBoardView {
    pub fn lane(&self, column: BoardColumn) -> Option<&BoardLane> {
        self.lanes.iter().find(|lane| lane.column == column)
    }

    pub fn card(&self, sidebar_session_id: &str) -> Option<&BoardCard> {
        self.lanes
            .iter()
            .flat_map(|lane| &lane.cards)
            .find(|card| card.sidebar_session_id == sidebar_session_id)
    }

    /// The title the filter shows: the project's, after its collection, or "All projects".
    pub fn filter_title(&self) -> String {
        match &self.filter {
            BoardFilter::All => "All projects".to_string(),
            BoardFilter::Project(project_id) => self
                .projects
                .iter()
                .find(|project| &project.project_id == project_id)
                .map_or_else(|| "All projects".to_string(), BoardProject::label),
        }
    }
}

/// Where a thread works in git: its branch, and its worktree's name when its project is one.
///
/// CDXC:SessionBoard 2026-10-28 DECISION:
/// User (Ryan): "Tambahkan informasi dari gitnya, misal agent ini kerja di branch atau worktree mana". A card names the branch the session reports, else its worktree's branch, and the worktree by name when the project is a worktree; a card with neither draws no git line.
fn git_place(
    session_branch: Option<&str>,
    worktree: Option<&WorktreeView>,
) -> (Option<String>, Option<String>) {
    let non_empty = |text: &str| Some(text.trim().to_string()).filter(|text| !text.is_empty());
    let branch = session_branch
        .and_then(non_empty)
        .or_else(|| worktree.and_then(|worktree| non_empty(&worktree.branch)));
    let worktree = worktree.and_then(|worktree| non_empty(&worktree.name));
    (branch, worktree)
}

fn card_status(column: BoardColumn, row: &SessionRow, starting: bool, sleeping: bool) -> BoardCardStatus {
    let facts = bucket_facts(row);
    if row.model_selection_failed || row.program_status_failed {
        BoardCardStatus::Error
    } else if column == BoardColumn::InProgress {
        BoardCardStatus::Working
    } else if starting {
        BoardCardStatus::Starting
    } else if facts.waits_on_answer() {
        BoardCardStatus::NeedsAnswer
    } else if facts.needs_you() {
        BoardCardStatus::Unseen
    } else if sleeping {
        BoardCardStatus::Sleeping
    } else {
        BoardCardStatus::None
    }
}

fn recency_ms(row: &SessionRow) -> i64 {
    row.timing
        .last_interaction_ms
        .or(row.timing.working_started_ms)
        .or(row.timing.created_ms)
        .unwrap_or(0)
}

/// Each lane's order: tasks being started lead Pending; Review is the triage order, answers
/// first, then unseen turns, then the most recent; Completed shows the latest completion first.
fn sort_lane(column: BoardColumn, cards: &mut [BoardCard]) {
    match column {
        BoardColumn::Pending => cards.sort_by_key(|card| {
            (
                card.status != BoardCardStatus::Starting,
                std::cmp::Reverse(card.row.timing.created_ms.unwrap_or(0)),
            )
        }),
        BoardColumn::InProgress => {
            cards.sort_by_key(|card| std::cmp::Reverse(recency_ms(&card.row)))
        }
        BoardColumn::Review => cards.sort_by_key(|card| {
            let rank = match card.status {
                BoardCardStatus::NeedsAnswer | BoardCardStatus::Error => 0,
                BoardCardStatus::Unseen => 1,
                _ => 2,
            };
            (rank, std::cmp::Reverse(recency_ms(&card.row)))
        }),
        BoardColumn::Completed => cards.sort_by(|left, right| {
            right
                .completed_at
                .cmp(&left.completed_at)
                .then_with(|| recency_ms(&right.row).cmp(&recency_ms(&left.row)))
        }),
    }
}

/// The Board for the window: the selected machine's selected Space, cut to `filter`.
pub fn build_session_board(
    core: &Core,
    inputs: &SidebarInputs,
    filter: &BoardFilter,
    now_ms: u64,
) -> SessionBoardView {
    let scope = board_scope(core, inputs, now_ms);
    let titles: BTreeMap<(&str, &str), &str> = scope
        .threads
        .iter()
        .filter_map(|thread| {
            let key = thread.row.key.as_ref()?;
            Some((
                (key.project_id.as_str(), key.session_id.as_str()),
                thread.row.display_title.as_str(),
            ))
        })
        .collect();
    let project_titles: BTreeMap<&str, &str> = scope
        .projects
        .iter()
        .map(|(project_id, title)| (project_id.as_str(), title.as_str()))
        .collect();

    let mut cards: Vec<BoardCard> = Vec::new();
    for thread in &scope.threads {
        let Some(column) = board_column(&thread.session, &thread.row) else {
            continue;
        };
        let Some(key) = thread.row.key.clone() else {
            continue;
        };
        let task_preview = thread
            .session
            .pending_task_preview
            .clone()
            .filter(|_| thread.session.is_draft);
        let starting = task_preview.is_some() && thread.session.pending_task_started_at.is_some();
        let status = card_status(
            column,
            &thread.row,
            starting,
            thread.session.lifecycle_state == LifecycleState::Sleeping,
        );
        let thread_of = thread.row.coordinator_parent.as_ref().and_then(|parent| {
            titles
                .get(&(parent.project_id.as_str(), parent.session_id.as_str()))
                .map(|title| title.to_string())
        });
        let worktree = scope.project_worktrees.get(&thread.project_id);
        let (branch, worktree) = git_place(thread.row.menu_facts.branch.as_deref(), worktree);
        cards.push(BoardCard {
            sidebar_session_id: thread.row.sidebar_session_id.clone(),
            column,
            status,
            project_title: project_titles
                .get(thread.project_id.as_str())
                .map_or_else(|| thread.project_id.clone(), |title| title.to_string()),
            project_group: scope.project_groups.get(&thread.project_id).cloned(),
            project_id: thread.project_id.clone(),
            thread_of,
            task_waiting: task_preview.is_some() && !starting,
            task_preview,
            completed_at: thread.session.completed_at.clone(),
            branch,
            worktree,
            row: thread.row.clone(),
            key,
        });
    }

    let projects: Vec<BoardProject> = scope
        .projects
        .iter()
        .map(|(project_id, title)| BoardProject {
            project_id: project_id.clone(),
            title: title.clone(),
            group: scope.project_groups.get(project_id).cloned(),
            path: scope.project_paths.get(project_id).cloned(),
            count: cards
                .iter()
                .filter(|card| &card.project_id == project_id)
                .count(),
        })
        .collect();
    let filter = match filter {
        BoardFilter::Project(project_id)
            if projects
                .iter()
                .any(|project| &project.project_id == project_id) =>
        {
            filter.clone()
        }
        _ => BoardFilter::All,
    };
    if let BoardFilter::Project(project_id) = &filter {
        cards.retain(|card| &card.project_id == project_id);
    }
    let total = cards.len();
    let lanes = BoardColumn::ORDER
        .iter()
        .map(|column| {
            let mut lane_cards: Vec<BoardCard> = cards
                .iter()
                .filter(|card| card.column == *column)
                .cloned()
                .collect();
            sort_lane(*column, &mut lane_cards);
            BoardLane {
                column: *column,
                cards: lane_cards,
            }
        })
        .collect();
    SessionBoardView {
        space_name: scope.space_name,
        spaces: scope.spaces,
        projects,
        filter,
        lanes,
        total,
    }
}

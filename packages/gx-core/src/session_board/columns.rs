//! Which of the Board's four columns a thread sits in.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): every thread passes through the Board in four columns, "Pending, In Progress, Review, dan Completed". A new thread (a draft from the sidebar or a task made on the Board) is Pending, a working agent is In progress, a finished turn or a question ("needs you") is Review, and the user marks reviewed threads Completed. Asked about threads that were already finished and seen, or slept by auto-sleep, without being marked completed, he chose Review, with a Complete all button on the Review column that skips threads waiting on an answer. Only agent threads in a workspace tab are cards: browser rows, plain terminals and command-pane sessions are not threads.

use ghostex_gx_protocol::{LifecycleState, PresentationSession, SessionKind, SessionSurface};

use crate::sidebar_view::{SessionBucketFacts, SessionRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BoardColumn {
    Pending,
    InProgress,
    Review,
    Completed,
}

impl BoardColumn {
    pub const ORDER: [BoardColumn; 4] = [
        BoardColumn::Pending,
        BoardColumn::InProgress,
        BoardColumn::Review,
        BoardColumn::Completed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BoardColumn::Pending => "Pending",
            BoardColumn::InProgress => "In progress",
            BoardColumn::Review => "Review",
            BoardColumn::Completed => "Completed",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BoardColumn::Pending => "pending",
            BoardColumn::InProgress => "inProgress",
            BoardColumn::Review => "review",
            BoardColumn::Completed => "completed",
        }
    }

    /// What an empty column says.
    pub fn empty_label(self) -> &'static str {
        match self {
            BoardColumn::Pending => "Nothing waiting",
            BoardColumn::InProgress => "Work happens here",
            BoardColumn::Review => "Nothing to review",
            BoardColumn::Completed => "Nothing completed",
        }
    }
}

/// The facts the sidebar's own status rules read, so "needs you" means the same on the Board.
pub fn bucket_facts(row: &SessionRow) -> SessionBucketFacts<'_> {
    SessionBucketFacts {
        activity: &row.activity,
        pending_question_count: row.pending_question_count,
        thread_state: row.thread_state.as_deref(),
        ..SessionBucketFacts::default()
    }
}

/// The column of one thread, or `None` when it is not a thread the Board shows.
pub fn board_column(session: &PresentationSession, row: &SessionRow) -> Option<BoardColumn> {
    if row.is_browser
        || session.kind != SessionKind::Agent
        || session.surface != SessionSurface::Workspace
        || session.lifecycle_state == LifecycleState::Stopped
    {
        return None;
    }
    if bucket_facts(row).is_running() {
        return Some(BoardColumn::InProgress);
    }
    if session
        .completed_at
        .as_deref()
        .is_some_and(|at| !at.trim().is_empty())
    {
        return Some(BoardColumn::Completed);
    }
    if session.is_draft || !session.has_ever_been_active {
        return Some(BoardColumn::Pending);
    }
    Some(BoardColumn::Review)
}

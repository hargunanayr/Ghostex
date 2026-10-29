//! The two-column sidebar's session buckets and status filters: which heading a session sits under
//! in the session column, and which sessions the Needs you and Running views list.
//!
//! CDXC:Sidebar 2026-10-27 DECISION:
//! User picked layout "A" for the sidebar: a navigation column (smart views Needs you / Running /
//! All sessions, then the projects) beside a session column that lists the selected project's
//! sessions under headings (Pinned, Needs you, Running, Today, This week, Older, Drafts, Parked)
//! with an All / Needs you / Running filter bar. It ships as the opt-in `sidebarLayout` setting
//! ("twoColumns"); Classic stays the default until the new layout reaches parity. "Needs you"
//! counts a session waiting on an answer (a question or a permission prompt, the pink dot) and a
//! finished turn the user has not seen yet (the blue dot), answers first. Older is a collapsible
//! heading that starts collapsed and lists every session once opened: no "Show N more" row
//! (CDXC:Projects 2026-10-13 in sections.rs still holds).
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/two_column/ (the renderer).

use super::text::parse_iso_ms;

/// `sidebarLayout`, the setting that picks the sidebar's layout.
pub const SIDEBAR_LAYOUT_SETTING_KEY: &str = "sidebarLayout";
pub const SIDEBAR_LAYOUT_CLASSIC: &str = "classic";
pub const SIDEBAR_LAYOUT_TWO_COLUMNS: &str = "twoColumns";

/// The saved `sidebarLayout`, with anything unknown read as Classic.
pub fn normalize_sidebar_layout(value: Option<&str>) -> &'static str {
    match value {
        Some(SIDEBAR_LAYOUT_TWO_COLUMNS) => SIDEBAR_LAYOUT_TWO_COLUMNS,
        _ => SIDEBAR_LAYOUT_CLASSIC,
    }
}

/// A heading of the two-column session list, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionBucket {
    Browser,
    Pinned,
    NeedsYou,
    Running,
    Today,
    ThisWeek,
    Older,
    Drafts,
    Parked,
}

impl SessionBucket {
    pub const ORDER: [SessionBucket; 9] = [
        SessionBucket::Browser,
        SessionBucket::Pinned,
        SessionBucket::NeedsYou,
        SessionBucket::Running,
        SessionBucket::Today,
        SessionBucket::ThisWeek,
        SessionBucket::Older,
        SessionBucket::Drafts,
        SessionBucket::Parked,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SessionBucket::Browser => "Browser",
            SessionBucket::Pinned => "Pinned",
            SessionBucket::NeedsYou => "Needs you",
            SessionBucket::Running => "Running",
            SessionBucket::Today => "Today",
            SessionBucket::ThisWeek => "This week",
            SessionBucket::Older => "Older",
            SessionBucket::Drafts => "Drafts",
            SessionBucket::Parked => "Parked",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SessionBucket::Browser => "browser",
            SessionBucket::Pinned => "pinned",
            SessionBucket::NeedsYou => "needsYou",
            SessionBucket::Running => "running",
            SessionBucket::Today => "today",
            SessionBucket::ThisWeek => "thisWeek",
            SessionBucket::Older => "older",
            SessionBucket::Drafts => "drafts",
            SessionBucket::Parked => "parked",
        }
    }

    /// Older and Parked start folded; the user opens them.
    pub fn starts_collapsed(self) -> bool {
        matches!(self, SessionBucket::Older | SessionBucket::Parked)
    }
}

/// The session column's filter bar, and the smart views that list every project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatusFilter {
    #[default]
    All,
    NeedsYou,
    Running,
}

impl StatusFilter {
    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All",
            StatusFilter::NeedsYou => "Needs you",
            StatusFilter::Running => "Running",
        }
    }
}

/// What a session row says about itself, as the snapshot carries it.
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionBucketFacts<'a> {
    /// `working`, `attention`, `idle`, ...
    pub activity: &'a str,
    pub pending_question_count: u64,
    /// gxserver's thread state; `waiting` is a thread blocked on the user.
    pub thread_state: Option<&'a str>,
    pub is_browser: bool,
    pub is_pinned: bool,
    /// The row sits in the Drafts heading (a draft with composer text, not pinned).
    pub is_draft_section: bool,
    /// Parked while parking is on, or snoozed.
    pub is_parked_section: bool,
    pub last_interaction_at: Option<&'a str>,
}

impl SessionBucketFacts<'_> {
    /// A question or a permission prompt is waiting on the user (the pink dot).
    pub fn waits_on_answer(&self) -> bool {
        self.pending_question_count > 0
            || (self.activity == "attention" && self.thread_state == Some("waiting"))
    }

    /// Waiting on an answer, or a finished turn the user has not seen (the blue dot).
    pub fn needs_you(&self) -> bool {
        self.waits_on_answer() || self.activity == "attention"
    }

    pub fn is_running(&self) -> bool {
        self.activity == "working"
    }

    pub fn matches(&self, filter: StatusFilter) -> bool {
        match filter {
            StatusFilter::All => true,
            StatusFilter::NeedsYou => self.needs_you(),
            StatusFilter::Running => self.is_running(),
        }
    }

    /// The heading the row sits under. `today_start_ms` is the start of the local day and
    /// `week_start_ms` seven days before now; the core reads no clock or time zone.
    pub fn bucket(&self, today_start_ms: i64, week_start_ms: i64) -> SessionBucket {
        if self.is_browser {
            return SessionBucket::Browser;
        }
        if self.is_parked_section {
            return SessionBucket::Parked;
        }
        if self.is_draft_section {
            return SessionBucket::Drafts;
        }
        if self.is_pinned {
            return SessionBucket::Pinned;
        }
        if self.needs_you() {
            return SessionBucket::NeedsYou;
        }
        if self.is_running() {
            return SessionBucket::Running;
        }
        match self.last_interaction_at.and_then(parse_iso_ms) {
            Some(at) if at >= today_start_ms => SessionBucket::Today,
            Some(at) if at >= week_start_ms => SessionBucket::ThisWeek,
            _ => SessionBucket::Older,
        }
    }
}

/// A coordinator and the threads drawn under it stay together under one heading: the most urgent
/// of their own, so a thread waiting on the user lifts its coordinator into Needs you. A pinned,
/// draft or parked coordinator keeps its heading.
pub fn family_bucket(members: impl IntoIterator<Item = SessionBucket>) -> SessionBucket {
    let mut members = members.into_iter();
    let Some(root) = members.next() else {
        return SessionBucket::Older;
    };
    if matches!(
        root,
        SessionBucket::Browser
            | SessionBucket::Pinned
            | SessionBucket::Drafts
            | SessionBucket::Parked
    ) {
        return root;
    }
    members
        .filter(|bucket| matches!(bucket, SessionBucket::NeedsYou | SessionBucket::Running))
        .fold(root, |best, bucket| best.min(bucket))
}

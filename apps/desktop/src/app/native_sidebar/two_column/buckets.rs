//! A project's rows sorted into the two-column headings, from the snapshot's session rows.

use std::collections::BTreeMap;
use std::sync::Arc;

use ghostex_gx_core::{SessionBucket, SessionBucketFacts, StatusFilter, family_bucket};
use serde_json::Value;

use super::super::model::{NativeSidebarGroup, NativeSidebarSession};

const DAY_MS: i64 = 86_400_000;

/// Where Today and This week start, read from the local clock once per frame.
#[derive(Clone, Copy)]
pub(super) struct DayBounds {
    pub(super) today_start_ms: i64,
    pub(super) week_start_ms: i64,
}

impl DayBounds {
    pub(super) fn now() -> Self {
        let now = chrono::Local::now();
        let now_ms = now.timestamp_millis();
        let today_start_ms = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|midnight| midnight.and_local_timezone(chrono::Local).earliest())
            .map_or(now_ms - DAY_MS, |midnight| midnight.timestamp_millis());
        Self {
            today_start_ms,
            week_start_ms: now_ms - 7 * DAY_MS,
        }
    }
}

fn flag(session: &NativeSidebarSession, key: &str) -> bool {
    session.details.get(key).and_then(Value::as_bool) == Some(true)
}

pub(super) fn facts(session: &NativeSidebarSession, parking: bool) -> SessionBucketFacts<'_> {
    SessionBucketFacts {
        activity: &session.activity,
        pending_question_count: session
            .details
            .get("pendingQuestionCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        thread_state: session.details.get("threadState").and_then(Value::as_str),
        is_browser: session.is_browser(),
        is_pinned: session.is_pinned,
        is_draft_section: session.is_draft && !session.is_pinned && session.has_composer_draft,
        is_parked_section: flag(session, "isSnoozed") || (parking && flag(session, "isParked")),
        last_interaction_at: session.last_interaction_at.as_deref(),
    }
}

/// A thread a folded coordinator hides, or one waiting behind its "N older threads" row.
fn hidden(session: &NativeSidebarSession) -> bool {
    flag(session, "threadHidden")
}

fn depth(session: &NativeSidebarSession) -> u64 {
    session
        .details
        .get("threadDepth")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Each top-level row with the threads drawn under it, in list order.
fn families(group: &NativeSidebarGroup) -> Vec<&[Arc<NativeSidebarSession>]> {
    let sessions = &group.sessions;
    let mut families = Vec::new();
    let mut start = 0;
    for index in 1..=sessions.len() {
        if index == sessions.len() || depth(&sessions[index]) == 0 {
            if start < index {
                families.push(&sessions[start..index]);
            }
            start = index;
        }
    }
    families
}

/// One heading of the project view and the rows under it.
pub(super) struct Bucketed {
    pub(super) bucket: SessionBucket,
    pub(super) rows: Vec<Arc<NativeSidebarSession>>,
}

struct Family {
    waits_on_answer: bool,
    rows: Vec<Arc<NativeSidebarSession>>,
}

/// The family's rows the list draws: the ones its tree shows and, where the list is Needs you,
/// every hidden thread that needs the user with the threads above it, so the tree still reaches it.
///
/// CDXC:Sidebar 2026-10-28 DECISION:
/// User, on finished threads missing from Needs you: a thread that needs the user is listed under Needs you even when its orchestrator is folded or the thread waits behind "N older threads" (a finished thread stops being recent two hours after its turn ended, gx-core sidebar_view/threads.rs). Under every other heading the fold and the older-threads row still hide it.
fn drawn_rows(
    members: &[Arc<NativeSidebarSession>],
    facts: &[SessionBucketFacts<'_>],
    lists_needs_you: bool,
) -> Vec<Arc<NativeSidebarSession>> {
    let mut keep: Vec<bool> = members.iter().map(|session| !hidden(session)).collect();
    let mut surfaced = false;
    for (index, session) in members.iter().enumerate() {
        if keep[index] || !lists_needs_you || !facts[index].needs_you() {
            continue;
        }
        surfaced = true;
        keep[index] = true;
        let mut wanted = depth(session);
        for above in (0..index).rev() {
            if wanted == 0 {
                break;
            }
            let above_depth = depth(&members[above]);
            if above_depth < wanted {
                keep[above] = true;
                wanted = above_depth;
            }
        }
    }
    let mut rows: Vec<Arc<NativeSidebarSession>> = members
        .iter()
        .zip(keep)
        .filter_map(|(session, keep)| keep.then(|| session.clone()))
        .collect();
    if !surfaced {
        return rows;
    }
    // A surfaced thread moves where its siblings' tree lines end.
    for index in 0..rows.len() {
        let own = depth(&rows[index]);
        if own == 0 {
            continue;
        }
        let last = rows[index + 1..]
            .iter()
            .map(|row| depth(row))
            .take_while(|row_depth| *row_depth >= own)
            .all(|row_depth| row_depth != own);
        if last != flag(&rows[index], "threadLast") {
            let mut row = (*rows[index]).clone();
            row.details
                .insert("threadLast".to_string(), Value::Bool(last));
            rows[index] = Arc::new(row);
        }
    }
    rows
}

fn matching_families(
    group: &NativeSidebarGroup,
    filter: StatusFilter,
    parking: bool,
    day: DayBounds,
) -> Vec<(SessionBucket, Family)> {
    families(group)
        .into_iter()
        .filter_map(|members| {
            let facts: Vec<SessionBucketFacts<'_>> = members
                .iter()
                .map(|session| facts(session, parking))
                .collect();
            if !facts.iter().any(|facts| facts.matches(filter)) {
                return None;
            }
            let bucket = family_bucket(
                facts
                    .iter()
                    .map(|facts| facts.bucket(day.today_start_ms, day.week_start_ms)),
            );
            let lists_needs_you =
                bucket == SessionBucket::NeedsYou || filter == StatusFilter::NeedsYou;
            Some((
                bucket,
                Family {
                    waits_on_answer: facts.iter().any(SessionBucketFacts::waits_on_answer),
                    rows: drawn_rows(members, &facts, lists_needs_you),
                },
            ))
        })
        .collect()
}

/// The project view: the rows matching `filter` under their headings, in heading order. Under
/// Needs you, a row waiting on an answer comes before a finished turn.
pub(super) fn bucket_group(
    group: &NativeSidebarGroup,
    filter: StatusFilter,
    parking: bool,
    day: DayBounds,
) -> Vec<Bucketed> {
    let mut by_bucket: BTreeMap<SessionBucket, Vec<Family>> = BTreeMap::new();
    for (bucket, family) in matching_families(group, filter, parking, day) {
        by_bucket.entry(bucket).or_default().push(family);
    }
    by_bucket
        .into_iter()
        .map(|(bucket, mut families)| {
            if bucket == SessionBucket::NeedsYou {
                families.sort_by_key(|family| !family.waits_on_answer);
            }
            Bucketed {
                bucket,
                rows: families
                    .into_iter()
                    .flat_map(|family| family.rows)
                    .collect(),
            }
        })
        .collect()
}

/// A smart view's rows of one project, answers first under Needs you.
pub(super) fn matching_rows(
    group: &NativeSidebarGroup,
    filter: StatusFilter,
    parking: bool,
    day: DayBounds,
) -> Vec<Arc<NativeSidebarSession>> {
    let mut families: Vec<Family> = matching_families(group, filter, parking, day)
        .into_iter()
        .map(|(_, family)| family)
        .collect();
    if filter == StatusFilter::NeedsYou {
        families.sort_by_key(|family| !family.waits_on_answer);
    }
    families
        .into_iter()
        .flat_map(|family| family.rows)
        .collect()
}

/// How many of a project's sessions each filter lists. Browser tabs are not sessions.
#[derive(Clone, Copy, Default)]
pub(super) struct Tally {
    pub(super) all: usize,
    pub(super) needs_you: usize,
    /// Of `needs_you`, the ones waiting on an answer (the pink dot).
    pub(super) waits_on_answer: usize,
    pub(super) running: usize,
}

impl Tally {
    pub(super) fn of(group: &NativeSidebarGroup, parking: bool) -> Self {
        let mut tally = Self::default();
        for session in group
            .sessions
            .iter()
            .filter(|session| !session.is_browser())
        {
            let facts = facts(session, parking);
            tally.all += 1;
            tally.needs_you += usize::from(facts.needs_you());
            tally.waits_on_answer += usize::from(facts.waits_on_answer());
            tally.running += usize::from(facts.is_running());
        }
        tally
    }

    pub(super) fn add(&mut self, other: Tally) {
        self.all += other.all;
        self.needs_you += other.needs_you;
        self.waits_on_answer += other.waits_on_answer;
        self.running += other.running;
    }

    pub(super) fn count(&self, filter: StatusFilter) -> usize {
        match filter {
            StatusFilter::All => self.all,
            StatusFilter::NeedsYou => self.needs_you,
            StatusFilter::Running => self.running,
        }
    }
}

//! The menu bar status item: which sessions count as working, waiting for the user or available,
//! per project.
//!
//! The rules are the old app runtime's (apps/desktop/sidebar/gxserver-runtime/helpers/status-indicators.ts),
//! over the same rows: every machine's sidebar groups (this computer's, then each saved remote
//! machine's, live or last seen), unfiltered by Space, tag or collapse, each in its display order.
//! The host keeps one [`SidebarViewModel`] per machine with neutral list state for that
//! (`neutral_indicator_inputs`) and hands over each one's `built_groups`.
//!
//! CDXC:StatusPet 2026-06-26-04:38:
//! GPUI status indicators consume the same saved shared Settings object as the
//! sidebar. Publish only bounded counts, booleans, and sidebar-projected project/session
//! ids/titles. Project icon parity may carry only an already-normalized image data URL for
//! notification attachments.
//!
//! CDXC:StatusPet 2026-06-27-20:11:
//! The standalone GPUI floating session indicator was removed. Keep producing status counts and
//! projects for the menu bar, but do not include floating visibility or floating
//! size settings in the status payload.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! The app runtime port (family F2) moves the derivation here, fed from the store, so the status
//! item no longer waits on the runtime's projection.

use serde_json::{Map, Value};

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::{SessionRow, SidebarInputs, SidebarViewModel};

/// `GPUI_STATUS_INDICATOR_MAX_CANDIDATES`.
const MAX_CANDIDATES: usize = 96;
/// `GPUI_STATUS_INDICATOR_MAX_PROJECTS`.
const MAX_PROJECTS: usize = 32;
/// `GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT`.
const MAX_SESSIONS_PER_PROJECT: usize = 16;
/// `GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS`.
const TITLE_MAX_CHARS: usize = 120;

pub const SESSION_STATUS_INDICATORS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.sessionStatusIndicators";

/// One row the status item may count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorCandidate {
    pub has_running_zmx_backing: bool,
    pub icon_data_url: Option<String>,
    pub last_interaction_at: Option<String>,
    pub order: usize,
    /// The session has an unanswered question card (the sidebar's pink dot).
    pub pending_question: bool,
    pub project_id: String,
    pub project_title: String,
    pub session_id: String,
    pub status: &'static str,
    pub title: String,
}

/// The list state the indicators read a machine's rows through: that machine selected, and no
/// Space, tag filter, collapse or hide applied, because the runtime counted its groups before any
/// of those.
pub fn neutral_indicator_inputs(list_inputs: &SidebarInputs, machine: &MachineId) -> SidebarInputs {
    let mut inputs = list_inputs.clone();
    inputs.ui = crate::sidebar_view::SidebarUiState {
        selected_machine_id: machine
            .remote_id()
            .unwrap_or(crate::sidebar_view::LOCAL_MACHINE_ID)
            .to_string(),
        show_all_projects: true,
        ..Default::default()
    };
    inputs.spaces_lifted = true;
    inputs
}

/// `boundedGpuiStatusIndicatorTitle`: trimmed-empty reads as the fallback, then at most 120
/// UTF-16 units.
fn bounded_title(value: Option<&str>, fallback: &str) -> String {
    let value = value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback);
    let mut out = String::new();
    let mut units = 0;
    for character in value.chars() {
        units += character.len_utf16();
        if units > TITLE_MAX_CHARS {
            break;
        }
        out.push(character);
    }
    out
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The session's title for the status item. The last resort is the session's number, its place in the
/// group before the display sort.
fn session_title(row: &SessionRow, session_number: usize) -> String {
    let number = session_number.to_string();
    let facts = &row.menu_facts;
    let title = non_blank(facts.raw_display_title.as_deref())
        .or_else(|| non_blank(facts.primary_title.as_deref()))
        .or_else(|| non_blank(facts.terminal_title.as_deref()))
        .or_else(|| non_blank(Some(row.alias.as_str())))
        .or(Some(number.as_str()));
    bounded_title(title, "Untitled session")
}

/// `hasRunningZmxBackingForGpuiIdleIndicator`: a terminal row on zmx whose provider session exists.
fn has_running_zmx_backing(core: &Core, row: &SessionRow) -> bool {
    if row.session_kind.as_deref() != Some("terminal") {
        return false;
    }
    let facts = &row.menu_facts;
    if facts.session_persistence_provider.as_deref() != Some("zmx")
        || non_blank(facts.session_persistence_name.as_deref()).is_none()
    {
        return false;
    }
    let Some(key) = &row.key else {
        return false;
    };
    let Some(session) = core
        .presentation()
        .machine(&key.machine)
        .and_then(|machine| machine.effective_session(&key.project_id, &key.session_id))
    else {
        return false;
    };
    let published = session.provider_session_state.as_str();
    let state = if !published.is_empty() {
        published
    } else {
        match session.lifecycle_state.as_str() {
            "running" => "exists",
            _ => "missing",
        }
    };
    state == "exists"
}

/// `createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups`, over each machine's groups in
/// the order the runtime built them.
pub fn indicator_candidates(
    core: &Core,
    models: &[(MachineId, &SidebarViewModel)],
) -> Vec<IndicatorCandidate> {
    let mut candidates = Vec::new();
    for (machine, model) in models {
        for (core_group, unsorted) in model.built_groups() {
            if candidates.len() >= MAX_CANDIDATES {
                return candidates;
            }
            let group_project = core_group.project_context.as_ref().map(|context| {
                ProjectKey {
                    machine: machine.clone(),
                    project_id: context.project_id.clone(),
                }
                .to_workspace_project_id()
            });
            let icon_data_url = core_group
                .project_context
                .as_ref()
                .and_then(|context| context.icon_data_url.clone())
                .filter(|url| {
                    url.starts_with("data:image/png;base64,")
                        || url.starts_with("data:image/svg+xml;base64,")
                });
            for session in &core_group.sessions {
                if candidates.len() >= MAX_CANDIDATES {
                    return candidates;
                }
                let row = &session.row;
                let session_number = unsorted
                    .iter()
                    .position(|candidate| candidate.sidebar_session_id == row.sidebar_session_id)
                    .unwrap_or_default()
                    + 1;
                // `parseGxserverPresentationProjectSessionId` reads only this computer's ids, so a
                // remote row outside a project group counts nowhere, as there.
                let row_project = row
                    .key
                    .as_ref()
                    .filter(|key| key.machine.is_local() && !row.is_browser)
                    .map(|key| key.project_id.clone());
                let Some(project_id) = group_project.clone().or(row_project) else {
                    continue;
                };
                let project_title = bounded_title(Some(core_group.title.as_str()), &project_id);
                candidates.push(IndicatorCandidate {
                    has_running_zmx_backing: has_running_zmx_backing(core, row),
                    icon_data_url: icon_data_url.clone(),
                    last_interaction_at: row.last_interaction_at.clone(),
                    order: candidates.len(),
                    pending_question: row.pending_question_count > 0,
                    project_id,
                    project_title,
                    session_id: row.sidebar_session_id.clone(),
                    // An approval waits on the user even under a title that settled to idle.
                    status: match row.activity.as_str() {
                        _ if row.awaiting_approval => "attention",
                        "attention" => "attention",
                        "working" => "working",
                        _ => "available",
                    },
                    title: session_title(row, session_number),
                });
            }
        }
    }
    candidates
}

/// `shouldCountGpuiSessionStatusIndicatorCandidate`.
fn counts(candidate: &IndicatorCandidate) -> bool {
    candidate.status != "available" || candidate.has_running_zmx_backing
}

fn status_counts(candidates: &[IndicatorCandidate]) -> (u64, u64, u64) {
    let (mut attention, mut available, mut working) = (0, 0, 0);
    for candidate in candidates.iter().filter(|candidate| counts(candidate)) {
        match candidate.status {
            "attention" => attention += 1,
            "working" => working += 1,
            _ => available += 1,
        }
    }
    (attention, available, working)
}

/// Working, waiting-for-you and asking-a-question counts split the way the sidebar's section
/// headers split them (`SectionView`): every working row counts as working, a row with an
/// unanswered question counts as a question and not as attention.
///
/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: the floating Ghostex Capture button shows three numbers, "working, attention, question
/// asked", for all local and remote sessions that currently have them.
fn section_counts(candidates: &[IndicatorCandidate]) -> Value {
    let (mut working, mut attention, mut question) = (0u64, 0u64, 0u64);
    for candidate in candidates {
        if candidate.pending_question {
            question += 1;
        } else if candidate.status == "attention" {
            attention += 1;
        }
        if candidate.status == "working" {
            working += 1;
        }
    }
    let mut counts = Map::new();
    counts.insert("attention".into(), Value::from(attention));
    counts.insert("question".into(), Value::from(question));
    counts.insert("working".into(), Value::from(working));
    Value::Object(counts)
}

/// `createGpuiSessionStatusIndicatorsPayload`.
pub fn status_indicators_payload(candidates: &[IndicatorCandidate], hide_menu_bar: bool) -> Value {
    let (attention, available, working) = status_counts(candidates);
    let mut projects: Vec<Map<String, Value>> = Vec::new();
    let mut project_ids: Vec<&str> = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| counts(candidate) || candidate.pending_question)
    {
        let index = match project_ids
            .iter()
            .position(|id| *id == candidate.project_id)
        {
            Some(index) => index,
            None => {
                if projects.len() >= MAX_PROJECTS {
                    continue;
                }
                let mut project = Map::new();
                if let Some(icon) = &candidate.icon_data_url {
                    project.insert("iconDataUrl".into(), Value::from(icon.as_str()));
                }
                project.insert(
                    "projectId".into(),
                    Value::from(candidate.project_id.as_str()),
                );
                project.insert("sessions".into(), Value::Array(Vec::new()));
                project.insert(
                    "title".into(),
                    Value::from(candidate.project_title.as_str()),
                );
                projects.push(project);
                project_ids.push(candidate.project_id.as_str());
                projects.len() - 1
            }
        };
        let Some(Value::Array(sessions)) = projects[index].get_mut("sessions") else {
            continue;
        };
        if sessions.len() >= MAX_SESSIONS_PER_PROJECT {
            continue;
        }
        let mut session = Map::new();
        if let Some(last) = &candidate.last_interaction_at {
            session.insert("lastActiveAt".into(), Value::from(last.as_str()));
        }
        if candidate.pending_question {
            session.insert("pendingQuestion".into(), Value::Bool(true));
        }
        session.insert(
            "sessionId".into(),
            Value::from(candidate.session_id.as_str()),
        );
        session.insert("sidebarOrder".into(), Value::from(candidate.order));
        session.insert("status".into(), Value::from(candidate.status));
        session.insert("title".into(), Value::from(candidate.title.as_str()));
        sessions.push(Value::Object(session));
    }
    let mut payload = Map::new();
    payload.insert("attentionCount".into(), Value::from(attention));
    payload.insert("availableCount".into(), Value::from(available));
    payload.insert("hideMenuBarIndicators".into(), Value::Bool(hide_menu_bar));
    payload.insert(
        "projects".into(),
        Value::Array(projects.into_iter().map(Value::Object).collect()),
    );
    payload.insert("sectionCounts".into(), section_counts(candidates));
    payload.insert(
        "type".into(),
        Value::from(SESSION_STATUS_INDICATORS_MESSAGE_TYPE),
    );
    payload.insert("version".into(), Value::from(1));
    payload.insert("workingCount".into(), Value::from(working));
    Value::Object(payload)
}

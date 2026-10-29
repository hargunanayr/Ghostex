//! A Ghost's habits: automations in its home project whose findings arrive in its chat.
//!
//! CDXC:Ghosts 2026-10-29 DECISION:
//! User chose to run habits on the Automate engine. A habit is an automation in the Ghost's home project (marked with `ghostId`), so the scheduler, run history, Run now and the Automate view all work for it; each run is a fresh session in the home folder, which gives it the Ghost's Soul and Memory. A run that reports findings (or needs attention) posts its final message to the Ghost's chat; a run with no findings stays quiet ("Nothing to report"), like Monocode's habits.
//! SEE-ALSO: server/src/automations/launch.rs (the habit prompt and title), server/src/automations/runtime.rs (`report_habit_run` after a run completes), server/src/server/ghost_habit_runtime.rs (delivery into the chat).

use std::sync::OnceLock;

use rusqlite::Connection;
use tokio::sync::mpsc::UnboundedSender;

use super::records::read_ghost_by_home_project;
use crate::domain::DomainRepository;

/// The largest report posted into a Ghost's chat.
const HABIT_REPORT_MAX_CHARS: usize = 12_000;

/// A finished habit run whose result goes to its Ghost's chat.
#[derive(Clone, Debug)]
pub struct GhostHabitReport {
    pub home_project_id: String,
    pub chat_session_id: String,
    pub habit_name: String,
    pub run_session_id: String,
    pub status: String,
    pub text: String,
}

static REPORT_SENDER: OnceLock<UnboundedSender<GhostHabitReport>> = OnceLock::new();

/// Set once at gxserver start by the runtime that delivers reports (it holds the app state).
pub fn set_habit_report_sender(sender: UnboundedSender<GhostHabitReport>) {
    let _ = REPORT_SENDER.set(sender);
}

/// The text typed into a habit run's session, ahead of Automate's result instructions.
pub fn ghost_habit_prompt(ghost_name: &str, habit_name: &str, instructions: &str) -> String {
    format!(
        "You are {ghost_name}, running your habit \"{habit_name}\" on its schedule. Nobody is watching this session: do the work below, then write the message you want the user to read in your chat (it is posted there as your habit report). \
If there is nothing worth telling the user this time, say so in one line and finish with no_findings, so nothing is posted. Do not start habits or change your Soul from here.\n\n{}",
        instructions.trim()
    )
}

/// The title a habit run's session gets.
pub fn ghost_habit_session_title(ghost_name: &str, habit_name: &str) -> String {
    format!("{ghost_name} · {habit_name}")
}

/// The Ghost whose habit an automation in `project_id` is.
pub fn habit_ghost_name(db: &Connection, project_id: &str) -> Option<String> {
    read_ghost_by_home_project(db, project_id)
        .ok()
        .flatten()
        .map(|ghost| ghost.name)
}

/// Strips Automate's result marker (and the summary under it) from the run's final message.
fn report_text(message: &str) -> String {
    let upper = message.to_ascii_uppercase();
    let body = match upper.rfind("AUTOMATION_RESULT:") {
        Some(index) => &message[..index],
        None => message,
    };
    body.trim().chars().take(HABIT_REPORT_MAX_CHARS).collect()
}

/// Hands a finished habit run to the delivery runtime. A quiet run (no findings) is not posted.
pub fn report_habit_run(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    habit_name: &str,
    run_session_id: &str,
    status: &str,
) {
    let Ok(Some(ghost)) = read_ghost_by_home_project(db, project_id) else {
        return;
    };
    let message = repository
        .get_session(project_id, run_session_id)
        .ok()
        .flatten()
        .and_then(|session| crate::notification_feed::body::last_assistant_message(&session))
        .map(|(text, _)| report_text(&text))
        .unwrap_or_default();
    let report = GhostHabitReport {
        home_project_id: ghost.home_project_id,
        chat_session_id: ghost.chat_session_id,
        habit_name: habit_name.to_string(),
        run_session_id: run_session_id.to_string(),
        status: status.to_string(),
        text: message,
    };
    if let Some(sender) = REPORT_SENDER.get() {
        let _ = sender.send(report);
    }
}

/// The message a report puts into the Ghost's chat, or `None` for a quiet run.
pub fn habit_report_message(report: &GhostHabitReport) -> Option<String> {
    match report.status.as_str() {
        "findings" => Some(format!(
            "Ghostex habit report: {}\n\n{}",
            report.habit_name,
            if report.text.is_empty() {
                "(The run found something but wrote no report.)"
            } else {
                report.text.as_str()
            }
        )),
        "needs_attention" | "failed" => Some(format!(
            "Ghostex habit report: {} could not finish.\n\n{}",
            report.habit_name,
            if report.text.is_empty() {
                "Open the habit's last run to see why."
            } else {
                report.text.as_str()
            }
        )),
        _ => None,
    }
}

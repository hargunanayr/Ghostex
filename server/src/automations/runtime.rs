use serde_json::Value;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::{
    domain::{DomainRepository, DomainStateError},
    paths::GxserverPaths,
    storage::open_gxserver_database,
};

use super::*;

#[derive(Clone)]
pub struct AutomationRuntime {
    pub(super) auth_token_file: String,
    pub(super) base_url: String,
    pub(super) paths: GxserverPaths,
    pub(super) server_id: String,
}

#[derive(Clone)]
pub(super) struct AutomationDefinitionRecord {
    pub(super) agent_id: String,
    pub(super) created_at: String,
    pub(super) enabled: bool,
    pub(super) execution_mode: Value,
    /// The Ghost this automation is a habit of (it lives in that Ghost's home project).
    pub(super) ghost_id: Option<String>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) next_run_at: Option<String>,
    pub(super) project_id: String,
    pub(super) prompt: String,
    pub(super) schedule: Value,
    pub(super) updated_at: String,
}

#[derive(Clone)]
pub(super) struct AutomationRunRecord {
    pub(super) automation_id: String,
    pub(super) completed_at: Option<String>,
    pub(super) created_at: String,
    pub(super) error_message: Option<String>,
    pub(super) findings_summary: Option<String>,
    pub(super) id: String,
    pub(super) is_archived: bool,
    pub(super) is_unread: bool,
    pub(super) project_id: String,
    pub(super) session_id: Option<String>,
    pub(super) status: String,
    pub(super) updated_at: String,
    pub(super) worktree: Value,
}

pub(super) struct AutomationLaunch {
    /*
    CDXC:Automations 2026-07-30:
    Launches that create a fresh agent session own an undelivered prompt.
    `createAgentSession` only records the prompt as `runtimeSettings.firstUserMessage`
    metadata, and the agent launch plan's `startupText` carries the agent command
    alone, so nothing types the prompt into the session. Carry the pending prompt
    out of the launch so the run watcher delivers it once the agent TUI is up.
    Thread launches leave this `None` because they already sent the prompt into an
    existing session.
    */
    pub(super) pending_prompt: Option<String>,
    pub(super) session_id: String,
    pub(super) session_project_id: String,
    pub(super) worktree: Option<Value>,
}

const AUTOMATION_SCHEDULER_TICK_SECONDS: u64 = 30;
const AUTOMATION_RUN_POLL_SECONDS: u64 = 5;
const AUTOMATION_RUN_POLL_LIMIT: usize = 720;
/*
CDXC:Automations 2026-07-30:
Mirror the GPUI sidebar's agent prompt contract (`GPUI_AGENT_PROMPT_READY_DELAY_MS`):
a freshly launched agent TUI needs a settle window before it accepts composer
input. The daemon waits inside the spawned run watcher, never inside the
scheduler tick or the `runAutomationNow` endpoint, so neither blocks.

CDXC:SessionChat 2026-08-26: the settle window is now the FALLBACK
for an agent with no measured composer signature; a signed agent is released as
soon as its input box appears. An automation is the case that suffered most from
the blind version — nobody is watching the pane, so a prompt typed into a boot
screen shows up only as a run that fails at the watcher timeout with no
AUTOMATION_RESULT marker.
*/
const AUTOMATION_PROMPT_READY_DELAY_SECONDS: u64 = 4;
/// Ceiling for the automation composer wait; matches the provider-startup one.
const AUTOMATION_PROMPT_COMPOSER_WAIT_TIMEOUT_MS: u64 = 10_000;
pub(super) const AUTOMATION_RESULT_PREFIX: &str = "AUTOMATION_RESULT:";
pub(super) const AUTOMATION_MAX_COUNT: usize = 500;
pub(super) const AUTOMATION_MAX_RUN_COUNT: usize = 5_000;

impl AutomationRuntime {
    pub fn new(paths: GxserverPaths, server_id: impl Into<String>, base_url: String) -> Self {
        Self {
            auth_token_file: paths.auth_token_file.to_string_lossy().to_string(),
            base_url,
            paths,
            server_id: server_id.into(),
        }
    }

    pub fn start(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        /*
        CDXC:Automations 2026-06-29-15:55:
        Automations are daemon-owned work now, not macOS renderer timers. Keep the scheduler loop in the gxserver automation module so CLI, macOS, GPUI, and remote clients share one durable SQLite source of truth and one runner while Ghostex is open.
        */
        let runtime = self.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(AUTOMATION_SCHEDULER_TICK_SECONDS));
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => {
                        let _ = runtime.run_scheduler_tick();
                    }
                }
            }
        });
    }

    fn run_scheduler_tick(&self) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        recover_running_automation_runs(self, &repository, &db)?;
        let due = read_due_automations(&db)?;
        for automation in due {
            if has_active_run(&db, &automation.id)? {
                let run = create_run_record(
                    &automation,
                    "skipped",
                    Some("Skipped because another run for this automation is still active."),
                );
                upsert_run(&db, &run)?;
                update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
                continue;
            }
            let _ = queue_automation_run(self, &repository, &db, &automation);
            update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
        }
        Ok(())
    }

    pub(super) fn spawn_run_watcher(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<String>,
    ) {
        let runtime = self.clone();
        tokio::spawn(async move {
            let _ = runtime
                .watch_automation_run(
                    project_id,
                    run_id,
                    session_project_id,
                    session_id,
                    pending_prompt,
                )
                .await;
        });
    }

    async fn watch_automation_run(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<String>,
    ) -> Result<(), DomainStateError> {
        if let Some(prompt) = pending_prompt {
            if let Err(error) = self
                .deliver_automation_prompt(&session_project_id, &session_id, &prompt)
                .await
            {
                let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
                return fail_run(&db, &project_id, &run_id, &error.message, "failed");
            }
        }
        let run_created_at = {
            let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
            read_run(&db, &project_id, &run_id)
                .ok()
                .map(|run| run.created_at)
        };
        for _ in 0..AUTOMATION_RUN_POLL_LIMIT {
            tokio::time::sleep(Duration::from_secs(AUTOMATION_RUN_POLL_SECONDS)).await;
            let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
            let repository = DomainRepository::new(&db, self.server_id.as_str());
            if !is_run_active(&db, &project_id, &run_id)? {
                return Ok(());
            }
            if let Some((status, summary)) = read_automation_result_from_session(
                &repository,
                &session_project_id,
                &session_id,
                run_created_at.as_deref(),
            )? {
                complete_run(&db, &project_id, &run_id, &status, summary.as_deref())?;
                report_ghost_habit_run(&db, &repository, &project_id, &run_id, &session_id, &status);
                return Ok(());
            }
        }
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        fail_run(
            &db,
            &project_id,
            &run_id,
            "Automation did not report an AUTOMATION_RESULT marker before the watcher timeout.",
            "needs_attention",
        )?;
        report_ghost_habit_run(
            &db,
            &repository,
            &project_id,
            &run_id,
            &session_id,
            "needs_attention",
        );
        Ok(())
    }

    /*
    CDXC:Automations 2026-07-30:
    Deliver the automation prompt the same way the GPUI sidebar delivers a first
    user message: start the provider, let the agent TUI settle, then submit the
    text through `sendSessionMessage`. Without this the session launches its agent
    command and idles at an empty composer, so no run can ever emit an
    AUTOMATION_RESULT marker and every run fails at the watcher timeout.
    */
    async fn deliver_automation_prompt(
        &self,
        session_project_id: &str,
        session_id: &str,
        prompt: &str,
    ) -> Result<(), DomainStateError> {
        crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
            &self.paths,
            self.server_id.as_str(),
            session_project_id,
            session_id,
            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                settle_ms: 0,
                timeout_ms: AUTOMATION_PROMPT_COMPOSER_WAIT_TIMEOUT_MS,
                unknown_hold_ms: AUTOMATION_PROMPT_READY_DELAY_SECONDS * 1_000,
            },
        )
        .await;
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        send_automation_prompt(&repository, session_project_id, session_id, prompt)
    }
}

/// Hands a finished run of a Ghost's habit to the Ghost (server/src/ghosts/habits.rs); runs of
/// every other automation are left alone.
pub(super) fn report_ghost_habit_run(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    status: &str,
) {
    let Ok(run) = read_run(db, project_id, run_id) else {
        return;
    };
    let habit_name: Option<String> = db
        .query_row(
            "SELECT name FROM automations WHERE automationId = ?1 AND ghostId IS NOT NULL",
            [&run.automation_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(habit_name) = habit_name {
        crate::ghosts::report_habit_run(db, repository, project_id, &habit_name, session_id, status);
    }
}

/// CDXC:Automations 2026-10-29 WHY:
/// The prompt goes in the way a chat message does (composer wait, verified clear, bracketed paste checked on screen, a separate Enter, then a check that the agent took it), not through `sendSessionMessage`. That endpoint types the raw text and sends Enter 150ms later without looking, and Claude Code swallowed that Enter for a long multi-line habit prompt: the run sat at "running" with the prompt staged in the input box and no transcript until the watcher timed out (Ghost habit run, 2026-10-29).
pub(super) fn send_automation_prompt(
    repository: &DomainRepository<'_>,
    session_project_id: &str,
    session_id: &str,
    prompt: &str,
) -> Result<(), DomainStateError> {
    let session = repository
        .get_session(session_project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {session_project_id}/{session_id} does not exist."
            ))
        })?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
    let steps = crate::session_chat_send::build_session_chat_message_steps(
        agent.as_deref(),
        prompt,
        &[],
        false,
    );
    // The `automation` source attributes every byte to this caller in the input log, and
    // analytics counts it as an automation prompt rather than a `gx sendMessage` one.
    crate::session_chat_send::enqueue_session_write_sequence(
        &session,
        session_project_id,
        session_id,
        "automation",
        steps,
    )?;
    if let Some(source) = crate::telemetry::prompt_source_for_diagnostic_input_source("automation")
    {
        crate::telemetry::prompt_sent(&session, source);
    }
    Ok(())
}

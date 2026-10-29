use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{FlagValue, Flags};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};

use super::*;

pub(super) fn create_gxserver_quick_terminal(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let cwd = node_path_resolve(&match payload.get("cwd") {
        Some(value) if !value.is_null() => js_string(value),
        _ => cwd_string(),
    });
    let project_id: Option<Value> = if let Some(value) = flags.0.get("projectId") {
        Some(value.as_json())
    } else if let Some(value) = payload.get("projectId").filter(|value| !value.is_null()) {
        Some(value.clone())
    } else {
        let project = ensure_gxserver_project_for_path(&cwd, flags)?;
        if project.is_null() {
            // JS crashes here with a TypeError when result.project is missing.
            return Err(CliError::Other(
                "Cannot read properties of undefined (reading 'projectId')".to_string(),
            ));
        }
        project.get("projectId").cloned()
    };
    let mut inner = Map::new();
    if let Some(value) = payload.get("command") {
        inner.insert("command".to_string(), value.clone());
    }
    inner.insert("cwd".to_string(), json!(cwd));
    set_or_remove(&mut inner, "projectId", project_id);
    let title = match payload.get("title") {
        Some(value) if !value.is_null() => value.clone(),
        _ => {
            if js_truthy(payload.get("command")) {
                Value::String(js_slice_utf16(
                    &js_string(payload.get("command").expect("truthy command")),
                    80,
                ))
            } else {
                json!("Terminal")
            }
        }
    };
    inner.insert("title".to_string(), title);
    create_gxserver_session(&Value::Object(inner), flags)
}

pub(super) fn create_gxserver_session(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let project_id_value = payload
        .get("projectId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("projectId").map(FlagValue::as_json));
    let project_id = normalize_required_project_id(project_id_value, "create-session")?;
    let input = string_or_empty(payload.get("input")).trim().to_string();
    let mut launch_settings = Map::new();
    if let Some(value) = payload.get("command") {
        launch_settings.insert("startupCommand".to_string(), value.clone());
    }
    if !input.is_empty() {
        launch_settings.insert("startupText".to_string(), json!(input));
        /*
        CDXC:Cli 2026-09-27 WHY:
        gxserver runs startup text when it starts the provider only when the session marks it queued, the flag an agent launch plan sets. Without it `--start` (and the first open from the sidebar) brought up an empty shell and never ran `--input`.
        */
        launch_settings.insert(
            "runtimeRelevant".to_string(),
            json!({ "queueProviderStartupText": true }),
        );
    }
    let mut params = Map::new();
    if let Some(value) = payload.get("cwd") {
        params.insert("cwd".to_string(), value.clone());
    }
    params.insert("kind".to_string(), json!("terminal"));
    /*
    CDXC:SessionTitles 2026-06-23-08:40:
    Mobile and CLI create-session callers may provide first-message input, but
    server must remain the owner of first-prompt auto-name generation.
    Pass the prompt through as runtime metadata and startup text instead of
    generating or staging title commands in the CLI.
    */
    if !launch_settings.is_empty() {
        params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    }
    params.insert("projectId".to_string(), json!(project_id));
    if !input.is_empty() {
        params.insert(
            "runtimeSettings".to_string(),
            json!({ "firstUserMessage": input }),
        );
    }
    params.insert(
        "title".to_string(),
        if js_truthy(payload.get("title")) {
            payload.get("title").expect("truthy title").clone()
        } else {
            json!("Terminal")
        },
    );
    let created = rpc::call_gxserver_rpc("/api/createSession", &Value::Object(params), flags)?;
    if payload.get("start") != Some(&Value::Bool(true)) {
        return Ok(created);
    }
    /*
    CDXC:Cli 2026-07-04-17:05:
    `ghostex create-session --start` mirrors `create-agent`: gxserver rows are
    created lazily and the zmx provider only materializes once something starts
    it, so `send-text`/`send-message`/`read-text` fail with "session does not
    exist" until then. Orchestration callers pass --start so the terminal is
    live immediately without waking/focusing panes through the UI.
    */
    start_created_session_provider(created, flags)
}

pub(super) fn create_gxserver_agent_session(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let project_id_value = payload
        .get("projectId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("projectId").map(FlagValue::as_json));
    let project_id = normalize_required_project_id(project_id_value, "create-agent")?;
    let agent_id_value = payload
        .get("agentId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("agentId").map(FlagValue::as_json));
    let agent_id = match agent_id_value {
        Some(value) => js_string(&value).trim().to_string(),
        None => String::new(),
    };
    if agent_id.is_empty() {
        return Err(CliError::Other(
            "create-agent requires an agent id.".to_string(),
        ));
    }
    crate::ghostex_cli::agents::require_launchable_agent(
        &agent_id,
        flags.truthy("allowHidden"),
        flags,
    )?;
    /*
    CDXC:Cli 2026-06-19-15:55:
    `ghostex create-agent` is a spawn command for automation and agent
    orchestration, not just a row-creation helper. After creating the gxserver
    session, immediately ask gxserver to materialize the zmx provider so
    subsequent `send-message` targets a live agent process instead of a shell
    prompt.
    */
    let mut params = Map::new();
    params.insert("agentId".to_string(), json!(agent_id));
    params.insert("projectId".to_string(), json!(project_id));
    for (key, flag) in [("agentModel", "--model"), ("agentEffort", "--effort")] {
        match payload.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(value)) if !value.trim().is_empty() => {
                params.insert(key.to_string(), json!(value));
            }
            Some(_) => {
                return Err(CliError::Other(format!(
                    "create-agent {flag} needs a value."
                )))
            }
        }
    }
    // CDXC:AgentBox 2026-10-01 SEE-ALSO: `--run-on docker|hetzner|…|docker:<host>` is the CLI spelling of the create's `runLocation` (server/src/agentbox/location.rs).
    match payload.get("runOn") {
        None | Some(Value::Null) => {}
        Some(Value::String(value)) => {
            let location =
                crate::agentbox::run_location_from_cli(value).map_err(CliError::Other)?;
            if location != "local" {
                params.insert("runLocation".to_string(), json!(location));
            }
        }
        Some(_) => {
            return Err(CliError::Other(
                "create-agent --run-on needs a location such as local, docker or hetzner."
                    .to_string(),
            ))
        }
    }
    // CDXC:SessionChat 2026-09-09 SEE-ALSO:
    // Mobile uses --defer-start to open the durable draft immediately; its background attach owns provider startup.
    if flags.truthy("deferStart") {
        params.insert("draft".to_string(), json!(true));
    }
    // CDXC:SessionBoard 2026-10-28 SEE-ALSO: `--task` makes a Board task: a draft that waits in Pending with its prompt and no agent started, which `ghostex start-task` sends (server/src/session_board/pending_task.rs).
    let task = match payload.get("task") {
        None | Some(Value::Null) => None,
        Some(Value::String(task)) if !task.trim().is_empty() => Some(task.trim().to_string()),
        Some(_) => {
            return Err(CliError::Other(
                "create-agent --task needs the task's prompt.".to_string(),
            ))
        }
    };
    if let Some(task) = task.as_deref() {
        if payload.get("prompt").is_some_and(|prompt| !prompt.is_null()) {
            return Err(CliError::Other(
                "create-agent takes --task or --prompt, not both.".to_string(),
            ));
        }
        params.insert("draft".to_string(), json!(true));
        params.insert(
            crate::session_board::PENDING_TASK_PROMPT_KEY.to_string(),
            json!(task),
        );
    }
    // CDXC:Sessions 2026-10-09 SEE-ALSO: `--replace-empty-sessions` is the phone's new-session action; gxserver then closes the project's other fully empty sessions only when the `closeEmptySessionsOnNew` setting is on and the agent opens in Chat (server/src/empty_session_cleanup.rs). Agents and scripts leave it out.
    if flags.truthy("replaceEmptySessions") {
        params.insert(
            crate::empty_session_cleanup::REPLACE_EMPTY_SESSIONS_PARAM.to_string(),
            json!(true),
        );
    }
    /*
    CDXC:Drafts 2026-08-20:
    `--first-input-draft` is the opposite of a first user message: gxserver
    types the text into the new agent's composer once the provider starts and
    never submits it, so SSH-only clients can hand the user a mention such as
    `@/path/export.md ` to write their own prompt around. The value is passed
    verbatim — a trailing space separates the mention from what the user types.
    */
    if let Some(draft) = payload
        .get("firstInputDraft")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        params.insert(
            "runtimeSettings".to_string(),
            json!({ "firstUserInputDraft": draft }),
        );
    }
    // CDXC:ClaudeCloud 2026-10-23 SEE-ALSO: `--prompt` is a Claude cloud session's task (server/src/claude_cloud/create.rs keeps it in the session's record and its terminal starts the session with it); other agents get their first message through `send-session-chat-message`.
    if let Some(prompt) = payload
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        let cloud = params.get("agentId").and_then(Value::as_str) == Some("claude-cloud")
            || params.get("runLocation").and_then(Value::as_str) == Some("claude-cloud");
        if !cloud {
            return Err(CliError::Other(
                "create-agent --prompt is for Claude cloud sessions (create-agent claude-cloud). Send other agents their first message with send-session-chat-message.".to_string(),
            ));
        }
        let mut runtime = params
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime.insert("firstUserMessage".to_string(), json!(prompt));
        params.insert("runtimeSettings".to_string(), Value::Object(runtime));
    }
    let title = flags
        .0
        .get("title")
        .map(FlagValue::as_json)
        .or_else(|| payload.get("title").cloned());
    set_or_remove(&mut params, "title", title);
    let created = rpc::call_gxserver_rpc("/api/createAgentSession", &Value::Object(params), flags)?;
    if flags.truthy("deferStart") || task.is_some() {
        return Ok(created);
    }
    start_created_session_provider(created, flags)
}

/// Shared `startSessionProvider` follow-up used by create-session --start and
/// create-agent: returns `created` untouched when the session row is missing
/// projectId/sessionId, otherwise merges the provider result.
fn start_created_session_provider(created: Value, flags: &Flags) -> CliResult<Value> {
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    if !js_truthy(session.get("projectId")) || !js_truthy(session.get("sessionId")) {
        return Ok(created);
    }
    let provider = rpc::call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({
            "projectId": session.get("projectId").cloned().unwrap_or(Value::Null),
            "sessionId": session.get("sessionId").cloned().unwrap_or(Value::Null),
        }),
        flags,
    )?;
    let mut object = created.as_object().cloned().unwrap_or_default();
    object.insert(
        "session".to_string(),
        match provider.get("session") {
            Some(value) if !value.is_null() => value.clone(),
            _ => session,
        },
    );
    object.insert("provider".to_string(), provider);
    Ok(Value::Object(object))
}

/*
CDXC:AddProject 2026-07-30:
`ghostex clone-repository` is the blocking front end to gxserver's clone JOB
endpoints, because Ghostex mobile drives the Add Project flow over one SSH exec
and cannot hold a polling loop of its own. The daemon still owns the clone, the
project registration, and the presentation delta; the CLI only waits for the job
to leave `running` and reports the final job record.

The wait timeout never cancels the job. A clone that outlives the CLI's patience
is still a clone the user asked for, so the command returns the still-running
job with `waitTimedOut: true` and leaves it to finish server-side.
*/
pub(super) fn clone_repository_and_wait(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let target = rpc::resolve_gxserver_server_target(flags, payload)?;
    let started = rpc::request_gxserver_rpc(&target, "/api/startRepositoryClone", payload, flags)?;
    let Some(job_id) = started
        .get("job")
        .and_then(|job| job.get("jobId"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(started);
    };
    let wait_timeout_ms = flags
        .number("waitTimeoutMs")
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(900_000.0) as u64;
    let poll_interval = std::time::Duration::from_millis(500);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_timeout_ms);
    let poll_params = json!({ "jobId": job_id });
    let mut latest = started;
    loop {
        if std::time::Instant::now() >= deadline {
            let mut object = latest.as_object().cloned().unwrap_or_default();
            object.insert("waitTimedOut".to_string(), Value::Bool(true));
            crate::ghostex_cli::set_exit_code(1);
            return Ok(Value::Object(object));
        }
        std::thread::sleep(poll_interval);
        latest =
            rpc::request_gxserver_rpc(&target, "/api/readRepositoryCloneJob", &poll_params, flags)?;
        let state = latest
            .get("job")
            .and_then(|job| job.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("running");
        match state {
            "running" => {}
            "completed" => return Ok(latest),
            _ => {
                crate::ghostex_cli::set_exit_code(1);
                return Ok(latest);
            }
        }
    }
}

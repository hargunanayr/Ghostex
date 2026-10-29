use ghostex_gx_chat_core::menus::orchestrator_effort::{
    default_orchestrator_effort, orchestrator_efforts,
};
use serde_json::{json, Value};

use super::{acknowledge_threads, playbook, sleep_threads, threads};
use crate::ghostex_cli::{
    agents,
    args::{parse_args, Flags, ParsedArgs},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};

/// CDXC:Coordinators 2026-10-10 DECISION:
/// User: "let's please rename "Coordinator" to "Orchestrator" everywhere so it's clearer to everyone." `ghostex orchestrator` (and `--orchestrator <ref>`) is the documented name; `ghostex coordinator` and `--coordinator` stay as hidden aliases so running sessions, playbook copies and older installs keep working. Internal names (code, routes, JSON fields, saved data) keep "coordinator".
/// CDXC:Coordinators 2026-09-30 WHY:
/// One verb family for everything a coordinator (or a user steering one) does, so the coordinator's playbook can name exact commands and `ghostex coordinator --help` is the one page to read. Messaging and reading threads stay on the existing `ghostex agents send` and `read-session-chat`, which already carry the sender header and reply reference.
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let wants_help = args.is_empty()
        || args
            .iter()
            .any(|arg| arg == "-h" || arg == "--help" || arg == "help");
    if wants_help {
        print!("{}", include_str!("help.txt"));
        return Ok(());
    }
    let command = args[0].as_str();
    let parsed = parse_args(&args[1..]);
    match command {
        "guide" => playbook::guide(&parsed),
        "create" => create(&parsed),
        "promote" => promote(&parsed),
        "options" => options(&parsed),
        "list" => list(&parsed),
        "status" => status(&parsed),
        "start-thread" => threads::start_thread(&parsed),
        "link" => threads::link(&parsed),
        "unlink" => threads::unlink(&parsed),
        "rename" => threads::rename(&parsed),
        "rename-self" => threads::rename_self(&parsed),
        "resolve" | "reopen" => threads::set_resolved(&parsed, command == "resolve"),
        "sleep-inactive-threads" => sleep_threads::sleep_inactive_threads(&parsed),
        "acknowledge-threads" => acknowledge_threads::acknowledge_threads(&parsed),
        "remember" | "forget" | "set-goal" | "set-instructions" => update(command, &parsed),
        "edit" => edit(&parsed),
        "playbook" => playbook::show(&parsed),
        "set-playbook" => playbook::set(&parsed),
        other => Err(CliError::Other(format!(
            "Unknown orchestrator command: {other}. See ghostex orchestrator --help."
        ))),
    }
}

pub(in crate::ghostex_cli) fn flag_text(flags: &Flags, key: &str) -> Option<String> {
    flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(in crate::ghostex_cli) fn text_or_file(
    parsed: &ParsedArgs,
    text_key: &str,
) -> CliResult<Option<String>> {
    if let Some(path) = flag_text(&parsed.flags, "bodyFile") {
        return std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")));
    }
    Ok(flag_text(&parsed.flags, text_key))
}

/// The session a `--coordinator <ref>` (or `--thread` style positional) names, from the list.
pub(super) fn resolve_session(reference: &str, flags: &Flags) -> CliResult<(Value, Flags)> {
    let flags = agents::inventory_flags(flags, reference)?;
    let rows = sessions::fetch_session_list(&flags, false)?;
    let row = selector::resolve_one_listed_session(reference, &rows, &flags)?;
    Ok((row, flags))
}

/// A thread by reference, closed sessions included, for `resolve` and `reopen`.
pub(super) fn resolve_thread_session(reference: &str, flags: &Flags) -> CliResult<(Value, Flags)> {
    let flags = agents::inventory_flags(flags, reference)?;
    let row = selector::resolve_live_or_closed_session(reference, &flags)?;
    Ok((row, flags))
}

/// The coordinator a command acts on: `--orchestrator <ref>` (or the older `--coordinator <ref>`),
/// else the calling session.
pub(super) fn target_coordinator(parsed: &ParsedArgs) -> CliResult<(Value, Flags)> {
    let base = server_flags(&parsed.flags);
    match flag_text(&parsed.flags, "orchestrator")
        .or_else(|| flag_text(&parsed.flags, "coordinator"))
    {
        Some(reference) => resolve_session(&reference, &base),
        None => {
            let caller = agents::caller().map_err(|error| {
                CliError::Other(format!(
                    "{error} Outside an orchestrator session, pass --orchestrator <ref>."
                ))
            })?;
            let flags = agents::inventory_flags(&base, agents::text(&caller, "globalRef"))?;
            Ok((caller, flags))
        }
    }
}

/// Only the flags the RPC layer reads (the server choice); command flags stay out of it.
pub(in crate::ghostex_cli) fn server_flags(flags: &Flags) -> Flags {
    let mut result = Flags::default();
    if let Some(server) = flags.string_value("server") {
        result.insert_text("server", server);
    }
    result
}

pub(super) fn read_view(session: &Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc(
        "/api/readCoordinator",
        &json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
        }),
        flags,
    )
}

fn create(parsed: &ParsedArgs) -> CliResult<()> {
    let base = server_flags(&parsed.flags);
    let caller = agents::caller().ok();
    let project_id = match flag_text(&parsed.flags, "projectId") {
        Some(project_id) => project_id,
        None => caller
            .as_ref()
            .map(|caller| agents::text(caller, "projectId").to_string())
            .filter(|project| !project.is_empty())
            .ok_or_else(|| {
                CliError::Other(
                    "Pass --project-id <id> (ghostex sessions --json lists project ids).".into(),
                )
            })?,
    };
    let flags = match (flag_text(&parsed.flags, "projectId"), caller.as_ref()) {
        (None, Some(caller)) => agents::inventory_flags(&base, agents::text(caller, "globalRef"))?,
        _ => base,
    };
    let hud = call_gxserver_rpc("/api/readSidebarHud", &json!({}), &flags)?;
    let mut agent_rows = hud["agents"].as_array().cloned().unwrap_or_default();
    let agent_id = match flag_text(&parsed.flags, "agent") {
        Some(agent) => {
            let row = agents::require_launchable_agent(
                &agent,
                parsed.flags.truthy("allowHidden"),
                &flags,
            )?;
            // A hidden agent allowed by --allow-hidden comes from the Agents roster.
            if row.is_object()
                && !agent_rows
                    .iter()
                    .any(|row| agents::text(row, "agentId") == agent)
            {
                agent_rows.push(row);
            }
            agent
        }
        None => default_coordinator_agent(&agent_rows).ok_or_else(|| {
            CliError::Other(format!(
                "No {} agent is configured. Pass --agent <agent-id> from ghostex agents types.",
                crate::coordinators::coordinator_agent_families_text()
            ))
        })?,
    };
    let named = flag_text(&parsed.flags, "title");
    // CDXC:Coordinators 2026-10-03 SEE-ALSO: keeps_its_given_title in server/src/coordinators/title.rs; an unnamed coordinator's "Coordinator" is a placeholder the agent's first name may replace.
    let title_source = if named.is_some() {
        "user"
    } else {
        "placeholder"
    };
    let title = named.unwrap_or_else(|| "Orchestrator".to_string());
    let goal = flag_text(&parsed.flags, "goal").unwrap_or_default();
    let task = flag_text(&parsed.flags, "task").unwrap_or_default();
    let family = agent_rows
        .iter()
        .find(|row| agents::text(row, "agentId") == agent_id)
        .and_then(agent_family);
    if family == Some("zcode") && flag_text(&parsed.flags, "effort").is_some() {
        return Err(CliError::Other(
            "ZCode agents take no effort choice; drop --effort.".into(),
        ));
    }
    let created = call_gxserver_rpc(
        "/api/createAgentSession",
        &json!({
            "projectId": project_id,
            "agentId": agent_id,
            "launchSettings": launch_settings_for(&agent_rows, &agent_id),
            "title": title,
            "runtimeSettings": { "titleSource": title_source },
            "coordinator": { "goal": goal, "firstRequest": task },
        })
        .as_object()
        .map(|object| {
            let mut object = object.clone();
            // CDXC:Coordinators 2026-10-17 SEE-ALSO: packages/gx-chat-core/src/menus/orchestrator_effort.rs (the user's High/XHigh decision); without --effort an orchestrator starts on its agent's highest orchestrator level where its launch takes an effort. ZCode takes no effort choice; its --model rides agentModel and gxserver queues it as a `/model` line ahead of the first message.
            let effort = flag_text(&parsed.flags, "effort").or_else(|| {
                family
                    .and_then(crate::coordinators::default_orchestrator_effort)
                    .map(str::to_string)
            });
            if let Some(effort) = effort {
                object.insert("agentEffort".to_string(), json!(effort));
            }
            let model = flag_text(&parsed.flags, "model").or_else(|| {
                (family == Some("claude")).then(|| DEFAULT_CLAUDE_COORDINATOR_MODEL.to_string())
            });
            if let Some(model) = model {
                object.insert("agentModel".to_string(), json!(model));
            }
            Value::Object(object)
        })
        .unwrap_or_default(),
        &flags,
    )?;
    let session = created
        .get("session")
        .cloned()
        .ok_or_else(|| CliError::Other("Create response has no session.".into()))?;
    let reference = agents::text(&session, "globalRef").to_string();
    call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({"globalRef": reference, "projectId": session["projectId"], "sessionId": session["sessionId"]}),
        &flags,
    )
    .map_err(|error| {
        CliError::Other(format!(
            "Created orchestrator {reference}, but its agent did not start: {error}. Open it in Ghostex to retry; do not create another."
        ))
    })?;
    let mut result = json!({
        "ok": true,
        "status": "created",
        "globalRef": reference,
        "session": agents::summary(&session),
    });
    // gxserver queued the first message (the skill line, then the task) before it answered.
    if !task.is_empty() {
        result["taskStatus"] = json!("queued");
    }
    let role_error = agents::text(&created, "coordinatorRoleError").to_string();
    if !role_error.is_empty() {
        result["roleError"] = json!(role_error);
        eprintln!(
            "Its orchestrator skill or first message could not be set up ({role_error}). Send it: ghostex agents send {reference} \"{}\"",
            crate::coordinators::orchestrator_skill_line(family.unwrap_or_default())
        );
    }
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        println!("Created orchestrator \"{title}\" ({reference}).");
        println!("Talk to it in Ghostex, or send it work with: ghostex agents send {reference} \"<request>\"");
    }
    Ok(())
}

/// `promote [<session-ref>]`: makes an existing session of a supported agent (the calling session when
/// no ref is given) a coordinator without restarting or interrupting it; see
/// `promote_session_to_coordinator` in server/src/coordinators/promote.rs.
fn promote(parsed: &ParsedArgs) -> CliResult<()> {
    let base = server_flags(&parsed.flags);
    let (session, flags) = match parsed.rest.first().map(String::as_str) {
        Some(reference) => resolve_session(reference, &base)?,
        None => {
            let caller = agents::caller().map_err(|error| {
                CliError::Other(format!(
                    "{error} Outside an agent session, pass the session to promote: ghostex orchestrator promote <session-ref>."
                ))
            })?;
            let flags = agents::inventory_flags(&base, agents::text(&caller, "globalRef"))?;
            (caller, flags)
        }
    };
    let mut params = json!({
        "projectId": session["projectId"],
        "sessionId": session["sessionId"],
    });
    if let Some(goal) = flag_text(&parsed.flags, "goal") {
        params["goal"] = json!(goal);
    }
    let result = call_gxserver_rpc("/api/promoteCoordinator", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let reference = agents::text(&result, "globalRef");
    println!(
        "\"{}\" ({reference}) is now an orchestrator. Its running turn was not interrupted.",
        agents::text(&result, "title")
    );
    if result["playbookQueued"].as_bool() == Some(true) {
        println!("The orchestrator skill is installed and the message to read it is queued; it reaches the session once it is idle.");
    } else {
        println!(
            "Its orchestrator skill or message could not be set up ({}). Send it with: ghostex agents send {reference} \"Run ghostex orchestrator guide and follow it from now on.\"",
            agents::text(&result, "playbookError")
        );
    }
    println!("Sessions it started earlier are not its threads yet; adopt each with: ghostex orchestrator link <session-ref> --orchestrator {reference}");
    Ok(())
}

/// What a New Coordinator form offers: the launchers an orchestrator can run on (Claude first, in launcher
/// order), each with its model lineup from the catalog gxserver serves, the model it starts on, and
/// the efforts each model accepts.
///
/// CDXC:Coordinators 2026-10-03 WHY:
/// The phone's New Coordinator form reads its choices here instead of parsing the model catalog itself, so its lineup is the one gxserver serves and its defaults are the ones `create` applies (Opus 5.5 for Claude, XHigh effort, the orchestrator levels only, and the locked skill line each agent's first message starts with).
/// SEE-ALSO: apps/desktop/src/app/gx_store/create/coordinator.rs (the desktop dialog's lineup), apps/mobile/app/src/screens/sessions-screen/NewCoordinatorSheet.tsx (the phone's form).
fn options(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = server_flags(&parsed.flags);
    let hud = call_gxserver_rpc("/api/readSidebarHud", &json!({}), &flags)?;
    let paths = crate::paths::get_gxserver_paths(None);
    crate::agent_model_catalog::adopt_cached_copy(&paths);
    let catalog = crate::agent_model_catalog::current();
    let all_efforts =
        crate::session_chat_catch_up::chat_setting_enabled(&paths, "orchestratorAllEfforts");
    let effort_label = |effort: &str| {
        catalog
            .pointer(&format!("/effortLabels/{effort}"))
            .and_then(Value::as_str)
            .unwrap_or(effort)
            .to_string()
    };
    let mut agents: Vec<(&str, Value)> = hud["agents"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let family = agent_family(row)?;
            // Only these agents' launches take a model (launch_plan.rs `apply_agent_model_options`);
            // the others start on their own configured model, so their form shows no model choice.
            let lineup = matches!(family, "claude" | "codex" | "pi" | "zcode" | "antigravity")
                .then(|| catalog.pointer(&format!("/agents/{family}")))
                .flatten();
            let agent_efforts = lineup
                .and_then(|agent| agent.get("efforts"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            let models: Vec<Value> = lineup
                .and_then(|agent| agent.get("models"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|model| {
                    let levels: Vec<&str> = model
                        .get("efforts")
                        .unwrap_or(&agent_efforts)
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_str)
                        .collect();
                    let offered = orchestrator_efforts(&levels, all_efforts, None);
                    let efforts: Vec<Value> = offered
                        .iter()
                        .map(|effort| json!({ "value": effort, "label": effort_label(effort) }))
                        .collect();
                    json!({
                        "value": model["value"],
                        "label": model["label"],
                        "default": model.get("default").and_then(Value::as_bool) == Some(true),
                        "efforts": efforts,
                        "defaultEffort": default_orchestrator_effort(&levels),
                    })
                })
                .collect();
            let default_model = models
                .iter()
                .find(|model| {
                    family == "claude" && model["value"] == DEFAULT_CLAUDE_COORDINATOR_MODEL
                })
                .or_else(|| models.iter().find(|model| model["default"] == true))
                .or_else(|| models.first())
                .map(|model| model["value"].clone())
                .unwrap_or(Value::Null);
            Some((
                family,
                json!({
                    "agentId": agents::text(row, "agentId"),
                    "name": row.get("name").and_then(Value::as_str).unwrap_or(family),
                    "family": family,
                    "models": models,
                    "defaultModel": default_model,
                    "skillLine": crate::coordinators::orchestrator_skill_line(family),
                }),
            ))
        })
        .collect();
    agents.sort_by_key(|(family, _)| *family != "claude");
    let agents: Vec<Value> = agents.into_iter().map(|(_, agent)| agent).collect();
    let result = json!({
        "agents": agents,
        // CDXC:Coordinators 2026-10-17 SEE-ALSO: packages/gx-chat-core/src/menus/orchestrator_effort.rs (the user's High/XHigh decision); each model's own `defaultEffort` is its highest offered level, and this is the level where the model has it.
        "defaultEffort": "xhigh",
    });
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        for agent in &agents {
            let models: Vec<&str> = agent["models"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|model| model["value"].as_str())
                .collect();
            println!(
                "{} ({}): {}",
                agents::text(agent, "name"),
                agents::text(agent, "agentId"),
                models.join(", ")
            );
        }
    }
    Ok(())
}

/// `{ agentCommand, icon }` of a launcher from `/api/readSidebarHud`, the way the desktop sends
/// it: custom agents are listed per machine, so a project that does not list one still launches it.
pub(in crate::ghostex_cli) fn launch_settings_for(rows: &[Value], agent_id: &str) -> Value {
    let mut settings = serde_json::Map::new();
    if let Some(row) = rows
        .iter()
        .find(|row| agents::text(row, "agentId") == agent_id)
    {
        for (from, to) in [("command", "agentCommand"), ("icon", "icon")] {
            let value = agents::text(row, from);
            if !value.is_empty() {
                settings.insert(to.to_string(), json!(value));
            }
        }
    }
    Value::Object(settings)
}

/// The first configured agent, in `COORDINATOR_AGENT_FAMILIES` order.
/// CDXC:Coordinators 2026-10-01 SEE-ALSO: DEFAULT_CLAUDE_COORDINATOR_MODEL in apps/desktop/src/app/window/new_coordinator_modal.rs (the user's Opus 5.5 decision); `create` on a Claude agent without `--model` uses the same, a Codex agent keeps its configured model.
pub(in crate::ghostex_cli) const DEFAULT_CLAUDE_COORDINATOR_MODEL: &str = "opus[1m]";

/// The orchestrator family of a launcher row (`COORDINATOR_AGENT_FAMILIES`): by its icon, its
/// agent id, or the executable its command runs.
pub(in crate::ghostex_cli) fn agent_family(row: &Value) -> Option<&'static str> {
    let icon = agents::text(row, "icon");
    let by_icon = crate::agents::default_agent_icon_to_id(icon).map(str::to_string);
    let by_id = Some(agents::text(row, "agentId").to_string());
    let by_command =
        crate::agents::infer_agent_id_from_command_executable(agents::text(row, "command"));
    [by_icon, by_id, by_command]
        .into_iter()
        .flatten()
        .find_map(|family| {
            crate::coordinators::COORDINATOR_AGENT_FAMILIES
                .into_iter()
                .find(|supported| *supported == family)
        })
}

pub(in crate::ghostex_cli) fn default_coordinator_agent(rows: &[Value]) -> Option<String> {
    crate::coordinators::COORDINATOR_AGENT_FAMILIES
        .iter()
        .find_map(|family| {
            rows.iter()
                .find(|row| agent_family(row) == Some(*family))
                .map(|row| agents::text(row, "agentId").to_string())
        })
}

fn list(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = server_flags(&parsed.flags);
    let mut params = json!({});
    if let Some(project_id) = flag_text(&parsed.flags, "projectId") {
        params["projectId"] = json!(project_id);
    }
    let result = call_gxserver_rpc("/api/listCoordinators", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let rows = result["coordinators"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        println!(
            "No orchestrators. Create one with: ghostex orchestrator create --title \"<name>\""
        );
        return Ok(());
    }
    for row in rows {
        let counts = row["threadCounts"]
            .as_object()
            .map(|counts| {
                counts
                    .iter()
                    .map(|(state, count)| format!("{count} {state}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "no threads".to_string());
        println!(
            "{}\t{}\t{}",
            agents::text(&row, "globalRef"),
            agents::text(&row, "title"),
            counts
        );
    }
    Ok(())
}

fn status(parsed: &ParsedArgs) -> CliResult<()> {
    let (session, flags) = target_coordinator(parsed)?;
    let view = read_view(&session, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    print_status(&view, parsed.flags.truthy("all"));
    Ok(())
}

fn first_line(text: &str, max: usize) -> String {
    crate::coordinators::report_headline(text, max)
}

pub(super) fn print_status(view: &Value, all: bool) {
    let coordinator = &view["coordinator"];
    let notice = agents::text(view, "playbookNotice");
    if !notice.is_empty() {
        println!("{notice}");
    }
    println!(
        "Orchestrator: {} ({})",
        agents::text(coordinator, "title"),
        agents::text(coordinator, "globalRef")
    );
    let goal = agents::text(coordinator, "goal");
    println!("Goal: {}", if goal.is_empty() { "(not set)" } else { goal });
    let instructions = agents::text(coordinator, "instructions");
    if instructions.is_empty() {
        println!("Standing instructions: (none)");
    } else {
        println!("Standing instructions:");
        for line in instructions.lines() {
            println!("  {line}");
        }
    }
    let memory = coordinator["memory"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if memory.is_empty() {
        println!("Memory: (no notes)");
    } else {
        println!("Memory:");
        for (index, note) in memory.iter().enumerate() {
            println!("  {}. {}", index + 1, agents::text(note, "text"));
        }
    }
    let threads = view["threads"].as_array().cloned().unwrap_or_default();
    if threads.is_empty() {
        println!("Threads: none yet. Start one with: ghostex orchestrator start-thread --title \"<title>\" --task \"<brief>\"");
        return;
    }
    println!("Threads:");
    for (state, heading) in [
        ("waiting", "Waiting on you"),
        ("finished", "Finished"),
        ("working", "Working"),
        ("sleeping", "Sleeping"),
        ("closed", "Closed"),
        ("done", "Done"),
    ] {
        let group = threads
            .iter()
            .filter(|thread| agents::text(thread, "state") == state)
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        if state == "done" && !all {
            println!(
                "  Done: {} (ghostex orchestrator status --all lists them)",
                group.len()
            );
            continue;
        }
        println!("  {heading}:");
        for thread in group {
            let title = agents::text(thread, "title");
            let mut line = format!(
                "    - {} [{}]",
                if title.is_empty() {
                    "(untitled)"
                } else {
                    title
                },
                agents::text(thread, "globalRef")
            );
            let branch = agents::text(thread, "branch");
            if !branch.is_empty() {
                line.push_str(&format!(" branch {branch}"));
            }
            println!("{line}");
            let waiting_for = agents::text(thread, "waitingFor");
            if !waiting_for.is_empty() {
                println!("      waiting for: {}", first_line(waiting_for, 160));
            }
            let report = agents::text(thread, "lastReport");
            if !report.is_empty() && state != "waiting" {
                println!("      last report: {}", first_line(report, 160));
            } else if report.is_empty() && state == "working" {
                println!(
                    "      task: {}",
                    first_line(agents::text(thread, "task"), 160)
                );
            }
        }
    }
}

/// `edit`: the goal, the instructions and the whole memory list in one call, the way the Goal &
/// Instructions and Memory editors on the computer and the phone save. `--goal=` and
/// `--instructions=` take an empty value to clear; `--notes-json=<JSON array of strings>` replaces
/// every note (notes kept keep their date).
fn edit(parsed: &ParsedArgs) -> CliResult<()> {
    let (session, flags) = target_coordinator(parsed)?;
    let mut params = json!({
        "projectId": session["projectId"],
        "sessionId": session["sessionId"],
    });
    for key in ["goal", "instructions"] {
        if let Some(value) = parsed.flags.string_value(key) {
            params[key] = json!(value);
        }
    }
    if let Some(notes) = parsed.flags.string_value("notesJson") {
        let notes: Vec<String> = serde_json::from_str(notes)
            .map_err(|_| CliError::Other("--notes-json takes a JSON array of strings.".into()))?;
        params["memory"] = json!(notes);
    }
    if params.as_object().map_or(0, |params| params.len()) == 2 {
        return Err(CliError::Other(
            "Usage: ghostex orchestrator edit [--goal=<text>] [--instructions=<text>] [--notes-json=<json>]".into(),
        ));
    }
    let view = call_gxserver_rpc("/api/updateCoordinator", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
    } else {
        println!("Orchestrator updated. Threads started from now on receive the change.");
    }
    Ok(())
}

fn update(command: &str, parsed: &ParsedArgs) -> CliResult<()> {
    let (session, flags) = target_coordinator(parsed)?;
    let value = parsed.rest.join(" ");
    let mut params = json!({
        "projectId": session["projectId"],
        "sessionId": session["sessionId"],
    });
    match command {
        "remember" => {
            if value.trim().is_empty() {
                return Err(CliError::Other(
                    "Usage: ghostex orchestrator remember \"<one line>\"".into(),
                ));
            }
            params["remember"] = json!(value);
        }
        "forget" => {
            let number = value
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|number| *number > 0)
                .ok_or_else(|| {
                    CliError::Other(
                        "Usage: ghostex orchestrator forget <number> (numbers come from ghostex orchestrator status).".into(),
                    )
                })?;
            params["forget"] = json!(number);
        }
        "set-goal" => params["goal"] = json!(value),
        _ => {
            let text = match text_or_file(parsed, "text")? {
                Some(text) => text,
                None => value,
            };
            params["instructions"] = json!(text);
        }
    }
    let view = call_gxserver_rpc("/api/updateCoordinator", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    match command {
        "remember" => {
            let count = view["coordinator"]["memory"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            println!("Saved as note {count}. Every new thread receives it.");
        }
        "forget" => println!("Note removed."),
        "set-goal" => println!("Goal updated."),
        _ => println!("Standing instructions updated. They reach threads started from now on."),
    }
    Ok(())
}

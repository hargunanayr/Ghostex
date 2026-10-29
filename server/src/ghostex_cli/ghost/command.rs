use serde_json::{json, Value};

use crate::ghostex_cli::{
    agents,
    args::{parse_args, Flags, FlagValue, ParsedArgs},
    coordinator,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// CDXC:Ghosts 2026-10-29 WHY:
/// One verb family for what a Ghost (or the user steering one) does, so its guide (server/src/ghosts/guide.md) names exact commands. Threads reuse the Orchestrator's start-thread, and following them up stays on `ghostex orchestrator` and `ghostex agents send`, because a Ghost's chat is an orchestrator underneath.
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
        "guide" => {
            print!("{}", crate::ghosts::GHOST_GUIDE);
            Ok(())
        }
        "list" => list(&parsed),
        "create" => create(&parsed),
        "whoami" => whoami(&parsed),
        "update" => update(&parsed),
        "delete" => delete(&parsed),
        "start-thread" => start_thread(&parsed),
        "memory" => memory(&parsed),
        "soul" => soul(&parsed),
        "skills" => skills(&parsed),
        "artifact" => super::artifacts::run(&parsed),
        "habit" => super::habits::run(&parsed),
        other => Err(CliError::Other(format!(
            "Unknown ghost command: {other}. See ghostex ghost --help."
        ))),
    }
}

pub(super) fn flags(parsed: &ParsedArgs) -> Flags {
    coordinator::server_flags(&parsed.flags)
}

pub(super) fn flag_text(parsed: &ParsedArgs, key: &str) -> Option<String> {
    coordinator::flag_text(&parsed.flags, key)
}

/// The Ghost a command acts on: `--ghost <name or id>`, else the Ghost whose home the calling
/// session runs in (its chat or a habit run). Returns the `/api/readGhost` view.
pub(super) fn target_ghost(parsed: &ParsedArgs) -> CliResult<Value> {
    let flags = flags(parsed);
    let params = match flag_text(parsed, "ghost") {
        Some(reference) => json!({ "ghostId": resolve_ghost_id(&reference, &flags)? }),
        None => {
            let caller = agents::caller().map_err(|error| {
                CliError::Other(format!(
                    "{error} Outside a Ghost's chat, pass --ghost <name or id>."
                ))
            })?;
            json!({ "projectId": agents::text(&caller, "projectId") })
        }
    };
    let view = call_gxserver_rpc("/api/readGhost", &params, &flags).map_err(|error| {
        CliError::Other(format!(
            "{error} Run this from a Ghost's chat, or pass --ghost <name or id>."
        ))
    })?;
    Ok(view["ghost"].clone())
}

fn resolve_ghost_id(reference: &str, flags: &Flags) -> CliResult<String> {
    let listed = call_gxserver_rpc("/api/listGhosts", &json!({}), flags)?;
    let ghosts = listed["ghosts"].as_array().cloned().unwrap_or_default();
    let wanted = reference.trim().to_lowercase();
    let matches = ghosts
        .iter()
        .filter(|ghost| {
            agents::text(ghost, "ghostId") == reference.trim()
                || agents::text(ghost, "name").to_lowercase() == wanted
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [ghost] => Ok(agents::text(ghost, "ghostId").to_string()),
        [] => Err(CliError::Other(format!(
            "No Ghost is named {reference}. ghostex ghost list shows them."
        ))),
        _ => Err(CliError::Other(format!(
            "Several Ghosts are named {reference}; pass its id from ghostex ghost list."
        ))),
    }
}

/// Project ids from `--projects a,b` (names or ids), checked against the project list.
fn resolve_project_ids(list: &str, flags: &Flags) -> CliResult<Vec<String>> {
    let listed = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let projects = listed["projects"].as_array().cloned().unwrap_or_default();
    list.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| resolve_project(item, &projects))
        .collect()
}

fn resolve_project(reference: &str, projects: &[Value]) -> CliResult<String> {
    let wanted = reference.to_lowercase();
    let matches = projects
        .iter()
        .filter(|project| {
            agents::text(project, "projectId") == reference
                || agents::text(project, "name").to_lowercase() == wanted
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [project] => Ok(agents::text(project, "projectId").to_string()),
        [] => Err(CliError::Other(format!(
            "No project is named {reference}. ghostex sessions --json lists project ids."
        ))),
        _ => Err(CliError::Other(format!(
            "Several projects are named {reference}; pass its project id."
        ))),
    }
}

fn print_ghost_line(ghost: &Value) {
    let projects = ghost["projects"]
        .as_array()
        .map(|projects| {
            projects
                .iter()
                .map(|project| agents::text(project, "name").to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    println!(
        "{}  {}  projects: {}",
        agents::text(ghost, "ghostId"),
        agents::text(ghost, "name"),
        if projects.is_empty() { "none" } else { &projects }
    );
}

fn list(parsed: &ParsedArgs) -> CliResult<()> {
    let listed = call_gxserver_rpc("/api/listGhosts", &json!({}), &flags(parsed))?;
    if parsed.flags.truthy("json") {
        print_json(&listed);
        return Ok(());
    }
    let ghosts = listed["ghosts"].as_array().cloned().unwrap_or_default();
    if ghosts.is_empty() {
        println!("No Ghosts yet. Create one with ghostex ghost create --name <name>.");
    }
    for ghost in &ghosts {
        print_ghost_line(ghost);
    }
    Ok(())
}

fn create(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = flags(parsed);
    let name = flag_text(parsed, "name")
        .ok_or_else(|| CliError::Other("create needs --name <name>.".into()))?;
    let hud = call_gxserver_rpc("/api/readSidebarHud", &json!({}), &flags)?;
    let mut agent_rows = hud["agents"].as_array().cloned().unwrap_or_default();
    let agent_id = match flag_text(parsed, "agent") {
        Some(agent) => {
            let row = agents::require_launchable_agent(&agent, parsed.flags.truthy("allowHidden"), &flags)?;
            if row.is_object()
                && !agent_rows
                    .iter()
                    .any(|row| agents::text(row, "agentId") == agent)
            {
                agent_rows.push(row);
            }
            agent
        }
        None => coordinator::default_coordinator_agent(&agent_rows).ok_or_else(|| {
            CliError::Other("No agent a Ghost can run on is configured. Pass --agent <agent-id> from ghostex agents types.".into())
        })?,
    };
    let family = agent_rows
        .iter()
        .find(|row| agents::text(row, "agentId") == agent_id)
        .and_then(coordinator::agent_family);
    let mut params = json!({
        "name": name,
        "agentId": agent_id,
        "launchSettings": coordinator::launch_settings_for(&agent_rows, &agent_id),
        "mascot": flag_text(parsed, "mascot").unwrap_or_default(),
        "color": flag_text(parsed, "color").unwrap_or_default(),
    });
    if let Some(list) = flag_text(parsed, "projects") {
        params["projectIds"] = json!(resolve_project_ids(&list, &flags)?);
    }
    let model = flag_text(parsed, "model").or_else(|| {
        (family == Some("claude")).then(|| coordinator::DEFAULT_CLAUDE_COORDINATOR_MODEL.to_string())
    });
    if let Some(model) = model {
        params["agentModel"] = json!(model);
    }
    if let Some(effort) = flag_text(parsed, "effort") {
        params["agentEffort"] = json!(effort);
    }
    let created = call_gxserver_rpc("/api/createGhost", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&created);
    } else {
        println!(
            "Created Ghost \"{}\" ({}). Open it from the Ghosts section of the sidebar.",
            agents::text(&created["ghost"], "name"),
            agents::text(&created["ghost"], "ghostId")
        );
    }
    Ok(())
}

fn whoami(parsed: &ParsedArgs) -> CliResult<()> {
    let ghost = target_ghost(parsed)?;
    if parsed.flags.truthy("json") {
        print_json(&ghost);
        return Ok(());
    }
    println!("You are {} ({}).", agents::text(&ghost, "name"), agents::text(&ghost, "ghostId"));
    println!("Home: {}", agents::text(&ghost, "homePath"));
    println!("Projects you hold:");
    let projects = ghost["projects"].as_array().cloned().unwrap_or_default();
    if projects.is_empty() {
        println!("  none yet (the user adds them in your settings)");
    }
    for project in projects {
        println!(
            "  {}  {}  ({})",
            agents::text(&project, "projectId"),
            agents::text(&project, "name"),
            agents::text(&project, "path")
        );
    }
    Ok(())
}

fn update(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = flags(parsed);
    let ghost = target_ghost(parsed)?;
    let mut params = json!({ "ghostId": ghost["ghostId"] });
    for key in ["name", "mascot", "color"] {
        if let Some(value) = parsed.flags.string_value(key) {
            params[key] = json!(value);
        }
    }
    if let Some(list) = parsed.flags.string_value("projects") {
        params["projectIds"] = json!(resolve_project_ids(list, &flags)?);
    }
    if let Some(show) = parsed.flags.string_value("showThreads") {
        params["showThreadsInSidebar"] = json!(show != "false");
    }
    let updated = call_gxserver_rpc("/api/updateGhost", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&updated);
    } else {
        print_ghost_line(&updated["ghost"]);
    }
    Ok(())
}

fn delete(parsed: &ParsedArgs) -> CliResult<()> {
    if flag_text(parsed, "ghost").is_none() {
        return Err(CliError::Other(
            "delete needs --ghost <name or id>, so a Ghost never deletes itself by accident.".into(),
        ));
    }
    let ghost = target_ghost(parsed)?;
    let deleted = call_gxserver_rpc(
        "/api/deleteGhost",
        &json!({ "ghostId": ghost["ghostId"] }),
        &flags(parsed),
    )?;
    if parsed.flags.truthy("json") {
        print_json(&deleted);
    } else {
        println!(
            "Deleted Ghost \"{}\". Its Soul and Memory stay in {}.",
            agents::text(&ghost, "name"),
            agents::text(&deleted, "homePath")
        );
    }
    Ok(())
}

/// The Orchestrator's start-thread, aimed at one of the Ghost's projects.
fn start_thread(parsed: &ParsedArgs) -> CliResult<()> {
    let ghost = target_ghost(parsed)?;
    let reference = flag_text(parsed, "project").ok_or_else(|| {
        CliError::Other("start-thread needs --project <name or id> (one of yours; ghostex ghost whoami lists them).".into())
    })?;
    let projects = ghost["projects"].as_array().cloned().unwrap_or_default();
    let project_id = resolve_project(&reference, &projects).map_err(|_| {
        CliError::Other(format!(
            "{} does not hold a project named {reference}. ghostex ghost whoami lists yours; ask the user to add it in your settings.",
            agents::text(&ghost, "name")
        ))
    })?;
    let mut forwarded = parsed.clone();
    forwarded
        .flags
        .insert_text("projectId", &project_id);
    forwarded.flags.0.remove("project");
    forwarded.flags.0.remove("ghost");
    let chat = &ghost["chatSession"];
    if flag_text(parsed, "ghost").is_some() && chat.is_object() {
        forwarded.flags.insert_text(
            "orchestrator",
            &format!(
                "{}:{}",
                agents::text(chat, "projectId"),
                agents::text(chat, "sessionId")
            ),
        );
    }
    if !forwarded.flags.0.contains_key("agent") {
        if let Some(agent) = chat.get("agentId").and_then(Value::as_str) {
            forwarded
                .flags
                .0
                .insert("agent".to_string(), FlagValue::Text(agent.to_string()));
        }
    }
    coordinator::start_thread(&forwarded)
}

fn memory(parsed: &ParsedArgs) -> CliResult<()> {
    let action = parsed.rest.first().map(String::as_str).unwrap_or("search");
    let flags = flags(parsed);
    let ghost = target_ghost(parsed)?;
    let words = parsed.rest.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
    let words = (!words.trim().is_empty()).then_some(words);
    if action == "read" {
        let file = match flag_text(parsed, "topic") {
            Some(topic) => format!("memory/{topic}.md"),
            None => "MEMORY.md".to_string(),
        };
        let view = call_gxserver_rpc(
            "/api/readGhost",
            &json!({ "ghostId": ghost["ghostId"] }),
            &flags,
        )?;
        if file == "MEMORY.md" {
            print!("{}", agents::text(&view["ghost"]["memory"], "text"));
        } else {
            let found = call_gxserver_rpc(
                "/api/ghostMemory",
                &json!({ "ghostId": ghost["ghostId"], "action": "search", "topic": flag_text(parsed, "topic") }),
                &flags,
            )?;
            for hit in found["matches"].as_array().into_iter().flatten() {
                if agents::text(hit, "file") == file {
                    println!("{}", agents::text(hit, "line"));
                }
            }
        }
        return Ok(());
    }
    let mut params = json!({ "ghostId": ghost["ghostId"], "action": action });
    match action {
        "add" | "replace" => {
            params["fact"] = json!(words.clone().ok_or_else(|| {
                CliError::Other(format!("memory {action} needs the fact in quotes."))
            })?);
        }
        "search" => {
            if let Some(words) = words.clone() {
                params["find"] = json!(words);
            }
        }
        "remove" => {}
        other => {
            return Err(CliError::Other(format!(
                "Unknown memory command: {other}. Use add, replace, remove, search or read."
            )))
        }
    }
    for key in ["find", "until", "topic"] {
        if let Some(value) = flag_text(parsed, key) {
            params[key] = json!(value);
        }
    }
    let result = call_gxserver_rpc("/api/ghostMemory", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    match action {
        "search" => {
            for hit in result["matches"].as_array().into_iter().flatten() {
                println!("{}: {}", agents::text(hit, "file"), agents::text(hit, "line"));
            }
        }
        "remove" => println!("Removed from {}.", agents::text(&result, "file")),
        _ => println!("Saved to {}: {}", agents::text(&result, "file"), agents::text(&result, "line")),
    }
    Ok(())
}

fn soul(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = flags(parsed);
    let ghost = target_ghost(parsed)?;
    match parsed.rest.first().map(String::as_str).unwrap_or("read") {
        "read" => {
            let view = call_gxserver_rpc(
                "/api/readGhost",
                &json!({ "ghostId": ghost["ghostId"] }),
                &flags,
            )?;
            let soul = &view["ghost"]["soul"];
            if parsed.flags.truthy("json") {
                print_json(soul);
            } else {
                print!("{}", agents::text(soul, "text"));
                eprintln!("(hash {})", agents::text(soul, "hash"));
            }
            Ok(())
        }
        "update" => {
            let text = match coordinator::text_or_file(parsed, "text")? {
                Some(text) => text,
                None => parsed.rest.iter().skip(1).cloned().collect::<Vec<_>>().join(" "),
            };
            if text.trim().is_empty() {
                return Err(CliError::Other(
                    "soul update needs --body-file <path> (or the text).".into(),
                ));
            }
            let expected = flag_text(parsed, "expectedHash").ok_or_else(|| {
                CliError::Other(
                    "soul update needs --expected-hash <hash> from ghostex ghost soul read, so the user's own edit is never overwritten.".into(),
                )
            })?;
            let result = call_gxserver_rpc(
                "/api/writeGhostFile",
                &json!({ "ghostId": ghost["ghostId"], "file": "SOUL.md", "text": text, "expectedHash": expected }),
                &flags,
            )?;
            println!("Soul saved (hash {}).", agents::text(&result, "hash"));
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown soul command: {other}. Use read or update."
        ))),
    }
}

fn skills(parsed: &ParsedArgs) -> CliResult<()> {
    let flags = flags(parsed);
    let ghost = target_ghost(parsed)?;
    let action = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let name = parsed.rest.get(1).cloned().unwrap_or_default();
    if matches!(action, "add" | "remove") && name.is_empty() {
        return Err(CliError::Other(format!("skills {action} needs a skill name.")));
    }
    let result = call_gxserver_rpc(
        "/api/ghostSkills",
        &json!({ "ghostId": ghost["ghostId"], "action": action, "name": name }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    println!("{}'s skills:", agents::text(&ghost, "name"));
    let own = result["skills"].as_array().cloned().unwrap_or_default();
    if own.is_empty() {
        println!("  none yet");
    }
    for skill in own {
        println!("  {}  {}", agents::text(&skill, "name"), agents::text(&skill, "description"));
    }
    if action == "list" {
        let available = result["available"].as_array().map(Vec::len).unwrap_or(0);
        println!("{available} installed skills can be added with ghostex ghost skills add <name>.");
    }
    Ok(())
}

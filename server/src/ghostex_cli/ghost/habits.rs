use serde_json::{json, Value};

use super::command::{flag_text, flags, target_ghost};
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    coordinator,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// `hourly :MM | daily HH:MM | weekdays HH:MM | weekly <day> HH:MM` as an Automate schedule. Times
/// are this computer's local time, like the Automate dialog's.
fn parse_schedule(text: &str) -> CliResult<Value> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    let time = |value: &str| -> CliResult<String> {
        let (hour, minute) = value
            .split_once(':')
            .and_then(|(hour, minute)| Some((hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?)))
            .filter(|(hour, minute)| *hour <= 23 && *minute <= 59)
            .ok_or_else(|| CliError::Other(format!("{value} is not a time like 17:20.")))?;
        Ok(format!("{hour:02}:{minute:02}"))
    };
    let bad = || {
        CliError::Other(
            "--schedule is \"hourly :00\", \"daily 17:20\", \"weekdays 09:00\" or \"weekly mon 09:00\".".into(),
        )
    };
    match words.as_slice() {
        ["hourly", minute] => {
            let minute = minute
                .trim_start_matches(':')
                .parse::<u32>()
                .ok()
                .filter(|minute| *minute <= 59)
                .ok_or_else(bad)?;
            Ok(json!({ "kind": "cron", "expression": format!("{minute} * * * *"), "timezone": "local" }))
        }
        ["daily", at] => Ok(json!({ "kind": "daily", "time": time(at)?, "timezone": "local" })),
        ["weekdays", at] => Ok(json!({ "kind": "weekly", "days": [1, 2, 3, 4, 5], "time": time(at)?, "timezone": "local" })),
        ["weekly", day, at] => {
            let day = DAYS
                .iter()
                .position(|name| day.to_lowercase().starts_with(name))
                .ok_or_else(bad)?;
            Ok(json!({ "kind": "weekly", "days": [day], "time": time(at)?, "timezone": "local" }))
        }
        _ => Err(bad()),
    }
}

/// A schedule as a person reads it ("Daily at 17:20").
fn schedule_label(schedule: &Value) -> String {
    let text = |key: &str| schedule.get(key).and_then(Value::as_str).unwrap_or_default();
    match text("kind") {
        "daily" => format!("Daily at {}", text("time")),
        "weekly" => {
            let days = schedule["days"]
                .as_array()
                .map(|days| days.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
                .unwrap_or_default();
            if days == [1, 2, 3, 4, 5] {
                format!("Weekdays at {}", text("time"))
            } else {
                let names = days
                    .iter()
                    .filter_map(|day| DAYS.get(*day as usize))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Weekly {names} at {}", text("time"))
            }
        }
        "cron" => match text("expression").split_whitespace().collect::<Vec<_>>().as_slice() {
            [minute, "*", "*", "*", "*"] => format!("Hourly at :{minute:0>2}"),
            _ => format!("Cron {}", text("expression")),
        },
        "interval" => "Repeating".to_string(),
        other => other.to_string(),
    }
}

fn automation_state(ghost: &Value, parsed: &ParsedArgs) -> CliResult<Value> {
    let state = call_gxserver_rpc(
        "/api/readAutomationState",
        &json!({ "projectId": ghost["homeProjectId"] }),
        &flags(parsed),
    )?;
    Ok(state["automationState"].clone())
}

fn find_habit(state: &Value, id: &str) -> CliResult<Value> {
    state["automations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|habit| agents::text(habit, "id") == id)
        .cloned()
        .ok_or_else(|| CliError::Other(format!("No habit has the id {id}. ghostex ghost habit list shows them.")))
}

/// `ghostex ghost habit list|add|update|run|remove`.
pub(super) fn run(parsed: &ParsedArgs) -> CliResult<()> {
    let ghost = target_ghost(parsed)?;
    let flags = flags(parsed);
    let action = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let home = ghost["homeProjectId"].clone();
    match action {
        "list" => {
            let state = automation_state(&ghost, parsed)?;
            if parsed.flags.truthy("json") {
                print_json(&state);
                return Ok(());
            }
            let habits = state["automations"].as_array().cloned().unwrap_or_default();
            if habits.is_empty() {
                println!("No habits yet.");
            }
            for habit in habits {
                println!(
                    "{}  {}  {}  {}{}",
                    agents::text(&habit, "id"),
                    agents::text(&habit, "name"),
                    schedule_label(&habit["schedule"]),
                    if habit["enabled"].as_bool() == Some(true) { "on" } else { "off" },
                    habit["nextRunAt"]
                        .as_str()
                        .map(|next| format!("  next {next}"))
                        .unwrap_or_default(),
                );
            }
            Ok(())
        }
        "add" | "update" => {
            let existing = if action == "update" {
                let id = parsed.rest.get(1).ok_or_else(|| {
                    CliError::Other("habit update needs the habit id from habit list.".into())
                })?;
                Some(find_habit(&automation_state(&ghost, parsed)?, id)?)
            } else {
                None
            };
            let mut definition = existing.clone().unwrap_or_else(|| {
                json!({
                    "agentId": ghost["chatSession"]["agentId"],
                    "enabled": true,
                    "executionMode": { "kind": "local" },
                })
            });
            if let Some(name) = flag_text(parsed, "name") {
                definition["name"] = json!(name);
            }
            if let Some(schedule) = flag_text(parsed, "schedule") {
                definition["schedule"] = parse_schedule(&schedule)?;
            }
            if let Some(instructions) = coordinator::text_or_file(parsed, "instructions")? {
                definition["prompt"] = json!(instructions);
            }
            if let Some(enabled) = flag_text(parsed, "enabled") {
                definition["enabled"] = json!(enabled != "false");
            }
            for (key, what) in [("name", "--name"), ("schedule", "--schedule"), ("prompt", "--instructions")] {
                if definition.get(key).is_none_or(Value::is_null) {
                    return Err(CliError::Other(format!("habit {action} needs {what}.")));
                }
            }
            // A changed schedule computes its own next run.
            if let Some(object) = definition.as_object_mut() {
                object.remove("nextRunAt");
            }
            let saved = call_gxserver_rpc(
                "/api/saveAutomation",
                &json!({ "projectId": home, "definition": definition }),
                &flags,
            )?;
            if parsed.flags.truthy("json") {
                print_json(&saved);
            } else {
                println!(
                    "{} habit \"{}\" ({}).",
                    if action == "add" { "Added" } else { "Updated" },
                    agents::text(&definition, "name"),
                    schedule_label(&definition["schedule"])
                );
            }
            Ok(())
        }
        "run" | "remove" => {
            let id = parsed.rest.get(1).ok_or_else(|| {
                CliError::Other(format!("habit {action} needs the habit id from habit list."))
            })?;
            let endpoint = if action == "run" {
                "/api/runAutomationNow"
            } else {
                "/api/deleteAutomation"
            };
            call_gxserver_rpc(endpoint, &json!({ "projectId": home, "automationId": id }), &flags)?;
            println!(
                "{}",
                if action == "run" {
                    "Started the habit. Its report arrives in your chat when it has something to tell."
                } else {
                    "Removed the habit."
                }
            );
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown habit command: {other}. Use list, add, update, run or remove."
        ))),
    }
}

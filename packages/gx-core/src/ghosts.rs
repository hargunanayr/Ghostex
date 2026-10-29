//! What every client says about a Ghost's habits: the schedule a person reads ("Daily at 17:20"),
//! what a run came to ("Messaged you", "Nothing to report"), and the schedule a person types.
//!
//! A habit is an automation in the Ghost's home project (server/src/ghosts/habits.rs), so its
//! schedule is an Automate schedule object and its runs are automation runs.

use serde_json::{Value, json};

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The schedule as a person reads it.
pub fn habit_schedule_label(schedule: &Value) -> String {
    let text = |key: &str| {
        schedule
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    match text("kind") {
        "daily" => format!("Daily at {}", text("time")),
        "weekly" => {
            let days = schedule
                .get("days")
                .and_then(Value::as_array)
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
                format!("Weekly on {names} at {}", text("time"))
            }
        }
        "cron" => match text("expression")
            .split_whitespace()
            .collect::<Vec<_>>()
            .as_slice()
        {
            [minute, "*", "*", "*", "*"] => format!("Hourly at :{minute:0>2}"),
            _ => format!("Cron {}", text("expression")),
        },
        "interval" => {
            let minutes = schedule.get("everyMs").and_then(Value::as_u64).unwrap_or(0) / 60_000;
            format!("Every {minutes} min")
        }
        "once" => "Once".to_string(),
        other => other.to_string(),
    }
}

/// What a run came to, in the words the Habits panel shows.
pub fn habit_run_outcome_label(status: &str) -> &'static str {
    match status {
        "findings" => "Messaged you",
        "no_findings" => "Nothing to report",
        "failed" | "needs_attention" => "Couldn't finish",
        "queued" | "running" => "Running",
        "skipped" => "Skipped",
        "cancelled" => "Cancelled",
        _ => "Unknown",
    }
}

/// Whether a run's outcome is drawn as good news (a report posted) rather than quiet or failed.
pub fn habit_run_posted(status: &str) -> bool {
    status == "findings"
}

/// Whether a run is still going (queued or running), so the habit shows as running and Run now
/// waits for it: the server skips a run started while another one is active.
pub fn habit_run_active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

/// A Ghost thread's state in the words the Threads list shows.
pub fn ghost_thread_state_label(state: &str) -> &'static str {
    match state {
        "waiting" => "Needs you",
        "working" => "Working",
        "finished" => "Finished",
        "done" => "Done",
        _ => "Idle",
    }
}

/// The schedule a person types: `hourly :MM`, `daily HH:MM`, `weekdays HH:MM` or
/// `weekly <day> HH:MM`, in this computer's time.
pub fn parse_habit_schedule(text: &str) -> Result<Value, String> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    let time = |value: &str| -> Result<String, String> {
        value
            .split_once(':')
            .and_then(|(hour, minute)| {
                Some((hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?))
            })
            .filter(|(hour, minute)| *hour <= 23 && *minute <= 59)
            .map(|(hour, minute)| format!("{hour:02}:{minute:02}"))
            .ok_or_else(|| format!("{value} is not a time like 17:20."))
    };
    let usage = || {
        "Write the schedule as \"hourly :00\", \"daily 17:20\", \"weekdays 09:00\" or \"weekly mon 09:00\"."
            .to_string()
    };
    match words.as_slice() {
        [kind, minute] if kind.eq_ignore_ascii_case("hourly") => {
            let minute = minute
                .trim_start_matches(':')
                .parse::<u32>()
                .ok()
                .filter(|minute| *minute <= 59)
                .ok_or_else(usage)?;
            Ok(
                json!({ "kind": "cron", "expression": format!("{minute} * * * *"), "timezone": "local" }),
            )
        }
        [kind, at] if kind.eq_ignore_ascii_case("daily") => {
            Ok(json!({ "kind": "daily", "time": time(at)?, "timezone": "local" }))
        }
        [kind, at] if kind.eq_ignore_ascii_case("weekdays") => Ok(
            json!({ "kind": "weekly", "days": [1, 2, 3, 4, 5], "time": time(at)?, "timezone": "local" }),
        ),
        [kind, day, at] if kind.eq_ignore_ascii_case("weekly") => {
            let day = DAYS
                .iter()
                .position(|name| day.to_lowercase().starts_with(&name.to_lowercase()))
                .ok_or_else(usage)?;
            Ok(json!({ "kind": "weekly", "days": [day], "time": time(at)?, "timezone": "local" }))
        }
        _ => Err(usage()),
    }
}

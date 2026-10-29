use std::collections::HashSet;

use chrono::{Datelike, NaiveDate, TimeZone, Utc};

#[cfg(test)]
use serde_json::json;
use serde_json::Value;

use crate::domain::DomainStateError;

#[cfg(test)]
use super::*;

pub(super) fn compute_next_run_at(
    schedule: &Value,
    after: Option<chrono::DateTime<Utc>>,
) -> Option<String> {
    let now = after.unwrap_or_else(Utc::now);
    match schedule.get("kind").and_then(Value::as_str)? {
        "once" => {
            let run_at = chrono::DateTime::parse_from_rfc3339(
                schedule.get("runAt").and_then(Value::as_str)?,
            )
            .ok()?
            .with_timezone(&Utc);
            (run_at > now).then(|| run_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "interval" => {
            let every_ms = schedule.get("everyMs").and_then(Value::as_i64)?;
            Some(
                (now + chrono::Duration::milliseconds(every_ms))
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            )
        }
        "daily" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            next_daily(now, hour, minute)
        }
        "weekly" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            let days = schedule.get("days").and_then(Value::as_array)?;
            next_weekly(
                now,
                hour,
                minute,
                days.iter().filter_map(Value::as_u64).collect(),
            )
        }
        "cron" => next_basic_cron(now, schedule.get("expression").and_then(Value::as_str)?),
        _ => None,
    }
}

pub(super) fn is_one_shot_schedule(schedule: &Value) -> bool {
    schedule.get("kind").and_then(Value::as_str) == Some("once")
}

fn next_daily(now: chrono::DateTime<Utc>, hour: u32, minute: u32) -> Option<String> {
    let local_today = now.with_timezone(&chrono::Local).date_naive();
    let today = candidate_local(local_today, hour, minute)?;
    let next = if today > now {
        today
    } else {
        candidate_local(local_today.succ_opt()?, hour, minute)?
    };
    Some(next.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// The next `:minute` of an hour after `now`, in this computer's time (a whole hour later when
/// that minute has passed).
fn next_hourly(now: chrono::DateTime<Utc>, minute: u32) -> Option<String> {
    use chrono::Timelike;
    let local = now.with_timezone(&chrono::Local);
    let this_hour = local
        .with_minute(minute)?
        .with_second(0)?
        .with_nanosecond(0)?
        .with_timezone(&Utc);
    let next = if this_hour > now {
        this_hour
    } else {
        this_hour + chrono::Duration::hours(1)
    };
    Some(next.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

fn next_weekly(
    now: chrono::DateTime<Utc>,
    hour: u32,
    minute: u32,
    days: Vec<u64>,
) -> Option<String> {
    let day_set = days.into_iter().collect::<HashSet<_>>();
    let local_today = now.with_timezone(&chrono::Local).date_naive();
    for offset in 0..=7 {
        let date = local_today.checked_add_signed(chrono::Duration::days(offset))?;
        let weekday = date.weekday().num_days_from_sunday() as u64;
        if !day_set.contains(&weekday) {
            continue;
        }
        let candidate = candidate_local(date, hour, minute)?;
        if candidate > now {
            return Some(candidate.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    None
}

fn next_basic_cron(now: chrono::DateTime<Utc>, expression: &str) -> Option<String> {
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 5 {
        return None;
    }
    if let Some(step) = parts[0]
        .strip_prefix("*/")
        .and_then(|value| value.parse::<i64>().ok())
    {
        if parts[1] == "*" && parts[2] == "*" && parts[3] == "*" && parts[4] == "*" && step > 0 {
            let current_minute = now.timestamp() / 60;
            let next_minute = ((current_minute / step) + 1) * step;
            return Utc
                .timestamp_opt(next_minute * 60, 0)
                .single()
                .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    let minute = parts[0].parse::<u32>().ok().filter(|minute| *minute <= 59)?;
    if parts[1..] == ["*", "*", "*", "*"] {
        return next_hourly(now, minute);
    }
    let hour = parts[1].parse::<u32>().ok()?;
    if parts[2] != "*" || parts[3] != "*" {
        return None;
    }
    if parts[4] == "*" {
        return next_daily(now, hour, minute);
    }
    let days = parts[4]
        .split(',')
        .filter_map(|day| day.parse::<u64>().ok())
        .collect::<Vec<_>>();
    next_weekly(now, hour, minute, days)
}

/// `hour:minute` on `date` in this computer's time zone, as a UTC instant.
///
/// CDXC:Automations 2026-10-29 WHY:
/// The Automate dialog sends a daily or weekly time as the user's local time (with the local zone name), and Ghost habits are written the same way ("Daily at 17:20"), but the times were read as UTC, so in Jakarta a 17:20 run fired at 00:20. gxserver runs on the user's computer, so its local zone is the zone those times were written in. On a DST jump the earlier of two times wins, and a time that does not exist that day is skipped to the next.
fn candidate_local(date: NaiveDate, hour: u32, minute: u32) -> Option<chrono::DateTime<Utc>> {
    chrono::Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0)?)
        .earliest()
        .map(|time| time.with_timezone(&Utc))
}

fn parse_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour <= 23 && minute <= 59).then_some((hour, minute))
}

pub(super) fn normalize_time(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value.ok_or_else(|| DomainStateError::bad_request("Schedule time is required."))?;
    parse_time(&value)
        .map(|_| value)
        .ok_or_else(|| DomainStateError::bad_request("Schedule time must be HH:mm."))
}

pub(super) fn normalize_run_at(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value
        .ok_or_else(|| DomainStateError::bad_request("One-time schedule runAt is required."))?;
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|run_at| {
            run_at
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .map_err(|_| {
            DomainStateError::bad_request("One-time schedule runAt must be an ISO 8601 date.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_input_is_anchored_as_a_one_time_schedule() {
        let before = Utc::now();
        let normalized = normalize_schedule(Some(&json!({
            "kind": "timer",
            "delayMs": 60_000,
        })))
        .expect("timer should normalize");
        let run_at = chrono::DateTime::parse_from_rfc3339(
            normalized
                .get("runAt")
                .and_then(Value::as_str)
                .expect("timer has runAt"),
        )
        .expect("runAt is ISO 8601")
        .with_timezone(&Utc);
        assert_eq!(normalized.get("kind").and_then(Value::as_str), Some("once"));
        assert!(run_at >= before + chrono::Duration::seconds(59));
        assert!(run_at <= Utc::now() + chrono::Duration::seconds(61));
    }

    #[test]
    fn one_time_schedule_has_no_next_run_after_its_deadline() {
        let schedule = json!({
            "kind": "once",
            "runAt": "2026-08-14T09:30:00.000Z",
        });
        let before = Utc.with_ymd_and_hms(2026, 8, 14, 9, 0, 0).single().unwrap();
        let after = Utc
            .with_ymd_and_hms(2026, 8, 14, 10, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            compute_next_run_at(&schedule, Some(before)).as_deref(),
            Some("2026-08-14T09:30:00.000Z")
        );
        assert_eq!(compute_next_run_at(&schedule, Some(after)), None);
    }

    #[test]
    fn prompt_instructions_never_parse_as_a_result() {
        /*
        CDXC:Automations 2026-07-30:
        Regression lock for the pair of bugs that made every delivered run report a
        bogus result: the prompt is echoed into the scanned scrollback, so the
        instruction block must not contain a parseable marker line, and the scan
        must not stop at the first occurrence.
        */
        let prompt = build_automation_prompt("Me fale quem é o NAP");
        assert!(prompt.contains(AUTOMATION_RESULT_PREFIX));
        assert_eq!(parse_automation_result(&prompt), None);
    }

    #[test]
    fn agent_marker_after_echoed_instructions_wins() {
        let session_text = format!(
            "{}\n\nI looked into it.\n\nAUTOMATION_RESULT: needs_attention\nNo NAP context found in the repo.\n",
            build_automation_prompt("Me fale quem é o NAP")
        );
        assert_eq!(
            parse_automation_result(&session_text),
            Some((
                "needs_attention".to_string(),
                Some("No NAP context found in the repo.".to_string())
            ))
        );
    }

    #[test]
    fn last_valid_marker_wins_over_earlier_ones() {
        let session_text = "AUTOMATION_RESULT: findings\nfirst pass\n\nAUTOMATION_RESULT: no_findings\nsecond pass\n";
        assert_eq!(
            parse_automation_result(session_text),
            Some(("no_findings".to_string(), Some("second pass".to_string())))
        );
    }

    #[test]
    fn wrapped_instruction_echo_still_never_parses() {
        /*
        The pane hard-wraps long lines, so the instruction text can break right
        after the marker. An empty or partial trailing line must not parse.
        */
        for wrapped in [
            "AUTOMATION_RESULT:\ncompleted by exactly one of these three words - findings,\n",
            "  AUTOMATION_RESULT: completed by exactly one of these three\n  words - findings, no_findings, or needs_attention.\n",
        ] {
            assert_eq!(parse_automation_result(wrapped), None, "{wrapped}");
        }
    }

    #[test]
    fn prompt_shows_the_marker_alone_on_its_line() {
        /*
        Regression lock for an observed live failure: prose placed right after the
        marker got copied into the agent's answer as
        `AUTOMATION_RESULT: completed no_findings`, which parses as nothing and
        sent the run back to the watcher timeout. The marker must appear only on a
        line of its own, followed by a placeholder and nothing else.
        */
        let prompt = build_automation_prompt("Check the deploy");
        let marker_line = prompt
            .lines()
            .find(|line| line.contains(AUTOMATION_RESULT_PREFIX))
            .expect("prompt names the marker");
        assert_eq!(marker_line.trim(), "AUTOMATION_RESULT: <status>");
        assert_eq!(
            prompt.matches(AUTOMATION_RESULT_PREFIX).count(),
            1,
            "one marker occurrence keeps the echo unambiguous"
        );
    }

    #[test]
    fn status_word_must_stand_alone_after_the_marker() {
        for drifted in [
            "AUTOMATION_RESULT: completed no_findings\nsummary\n",
            "AUTOMATION_RESULT: status findings\nsummary\n",
            "AUTOMATION_RESULT: <status>\nsummary\n",
        ] {
            assert_eq!(parse_automation_result(drifted), None, "{drifted}");
        }
    }

    #[test]
    fn missing_marker_leaves_the_run_pending() {
        assert_eq!(
            parse_automation_result("claude booted and is waiting at an empty composer"),
            None
        );
    }
}

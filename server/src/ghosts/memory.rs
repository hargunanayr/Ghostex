//! A Ghost's Memory: dated facts in MEMORY.md, written only through these operations.
//!
//! A fact line is `- YYYY-MM-DD · fact` with an optional ` · until YYYY-MM-DD`. A replaced fact is
//! kept struck through (`- ~~…~~`) so the history of what the Ghost believed stays readable; lines
//! without a date are the user's own and are never touched. When the file grows past the context
//! cap, struck and then oldest facts move to `memory/archive.md`.

use chrono::Local;

use super::files::{
    read_ghost_file, write_ghost_file, GHOST_FILE_CONFLICT, GHOST_MEMORY_ARCHIVE_FILE,
    GHOST_MEMORY_CONTEXT_MAX_LINES, GHOST_MEMORY_FILE,
};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

pub const GHOST_FACT_MAX_CHARS: usize = 1000;
/// Lines MEMORY.md is trimmed to when it outgrows the context cap.
const MEMORY_TRIM_TO_LINES: usize = GHOST_MEMORY_CONTEXT_MAX_LINES - 20;
const WRITE_ATTEMPTS: usize = 3;

const SECRET_PREFIXES: &[&str] = &[
    "sk-", "sk_live_", "rk_live_", "ghp_", "gho_", "ghs_", "ghu_", "github_pat_", "xoxb-", "xoxp-",
    "xoxa-", "glpat-", "AKIA", "AIza",
];

/// Replaces anything that looks like an API key or token with `[redacted]`: a Ghost's memory is
/// read back into every conversation and must never carry a credential.
pub fn redact_secrets(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let bare = word.trim_matches(|ch: char| "`'\"()[]{}<>,;".contains(ch));
            let looks_secret = bare.len() >= 16
                && SECRET_PREFIXES.iter().any(|prefix| bare.starts_with(prefix))
                && !bare.contains(char::is_whitespace);
            if looks_secret {
                word.replace(bare, "[redacted]")
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn clean_fact(fact: &str) -> Result<String, DomainStateError> {
    let fact = fact.split_whitespace().collect::<Vec<_>>().join(" ");
    if fact.is_empty() {
        return Err(DomainStateError::bad_request("The fact is empty."));
    }
    if fact.chars().count() > GHOST_FACT_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Keep a fact under {GHOST_FACT_MAX_CHARS} characters; put longer notes in a topic (--topic)."
        )));
    }
    Ok(redact_secrets(&fact))
}

fn valid_date(text: &str) -> bool {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
}

pub fn fact_line(today: &str, fact: &str, until: Option<&str>) -> String {
    match until {
        Some(until) => format!("- {today} · {fact} · until {until}"),
        None => format!("- {today} · {fact}"),
    }
}

/// A dated fact line Ghostex wrote (struck or not); the user's own lines are not facts.
fn is_fact_line(line: &str) -> bool {
    let body = line
        .strip_prefix("- ~~")
        .or_else(|| line.strip_prefix("- "))
        .unwrap_or_default();
    body.get(..10).is_some_and(valid_date)
}

fn is_struck(line: &str) -> bool {
    line.starts_with("- ~~")
}

fn matching_lines<'a>(memory: &'a str, find: &str) -> Vec<(usize, &'a str)> {
    let needle = find.to_lowercase();
    memory
        .lines()
        .enumerate()
        .filter(|(_, line)| is_fact_line(line) && !is_struck(line))
        .filter(|(_, line)| line.to_lowercase().contains(&needle))
        .collect()
}

fn one_match(memory: &str, find: &str) -> Result<usize, DomainStateError> {
    let find = find.trim();
    if find.is_empty() {
        return Err(DomainStateError::bad_request(
            "Name the fact with --find \"<words from it>\".",
        ));
    }
    let matches = matching_lines(memory, find);
    match matches.as_slice() {
        [(index, _)] => Ok(*index),
        [] => Err(DomainStateError::not_found(format!(
            "No fact in memory contains \"{find}\"."
        ))),
        many => Err(DomainStateError::bad_request(format!(
            "\"{find}\" matches {} facts; use more of the fact's words:\n{}",
            many.len(),
            many.iter()
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n")
        ))),
    }
}

pub fn add_fact(memory: &str, line: &str) -> String {
    let mut out = memory.trim_end().to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    out
}

pub fn replace_fact(memory: &str, find: &str, line: &str) -> Result<String, DomainStateError> {
    let index = one_match(memory, find)?;
    let mut lines = memory.lines().map(str::to_string).collect::<Vec<_>>();
    let old = lines[index].trim_start_matches("- ").to_string();
    lines[index] = format!("- ~~{old}~~");
    lines.insert(index + 1, line.to_string());
    Ok(lines.join("\n") + "\n")
}

pub fn remove_fact(memory: &str, find: &str) -> Result<String, DomainStateError> {
    let index = one_match(memory, find)?;
    let lines = memory
        .lines()
        .enumerate()
        .filter(|(at, _)| *at != index)
        .map(|(_, line)| line)
        .collect::<Vec<_>>();
    Ok(lines.join("\n") + "\n")
}

/// Splits MEMORY.md into what stays and what moves to the archive once it outgrows the context cap:
/// struck facts go first, then the oldest live facts. The user's own lines always stay.
pub fn trim_for_archive(memory: &str) -> (String, Vec<String>) {
    let lines = memory.lines().map(str::to_string).collect::<Vec<_>>();
    if lines.len() <= GHOST_MEMORY_CONTEXT_MAX_LINES {
        return (memory.to_string(), Vec::new());
    }
    let mut excess = lines.len() - MEMORY_TRIM_TO_LINES;
    let mut moved = vec![false; lines.len()];
    for pass_struck in [true, false] {
        for (index, line) in lines.iter().enumerate() {
            if excess == 0 {
                break;
            }
            if is_fact_line(line) && is_struck(line) == pass_struck && !moved[index] {
                moved[index] = true;
                excess -= 1;
            }
        }
    }
    let kept = lines
        .iter()
        .zip(&moved)
        .filter(|(_, moved)| !**moved)
        .map(|(line, _)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let archived = lines
        .into_iter()
        .zip(moved)
        .filter(|(_, moved)| *moved)
        .map(|(line, _)| line)
        .collect();
    (kept, archived)
}

/// Applies one change to MEMORY.md (or a topic file) with the file's hash as a guard, retrying when
/// the user saved in between, and archives what no longer fits.
fn edit_memory_file(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
    change: impl Fn(&str) -> Result<String, DomainStateError>,
) -> Result<String, DomainStateError> {
    let mut last_error = None;
    for _ in 0..WRITE_ATTEMPTS {
        let current = read_ghost_file(paths, ghost_id, name)?;
        let next = change(&current)?;
        let (next, archived) = if name == GHOST_MEMORY_FILE {
            trim_for_archive(&next)
        } else {
            (next, Vec::new())
        };
        match write_ghost_file(
            paths,
            ghost_id,
            name,
            &next,
            Some(&super::files::content_hash(&current)),
        ) {
            Ok(hash) => {
                if !archived.is_empty() {
                    let archive = read_ghost_file(paths, ghost_id, GHOST_MEMORY_ARCHIVE_FILE)?;
                    let archive = add_fact(&archive, &archived.join("\n"));
                    write_ghost_file(paths, ghost_id, GHOST_MEMORY_ARCHIVE_FILE, &archive, None)?;
                }
                return Ok(hash);
            }
            Err(error) if error.code == GHOST_FILE_CONFLICT => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| DomainStateError::bad_request("Memory changed; retry.")))
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn topic_file(topic: Option<&str>) -> String {
    match topic.map(str::trim).filter(|topic| !topic.is_empty()) {
        Some(topic) => format!("memory/{topic}.md"),
        None => GHOST_MEMORY_FILE.to_string(),
    }
}

/// The memory operations `/api/ghostMemory` takes: add, replace, remove, search.
pub fn apply_memory_action(
    paths: &GxserverPaths,
    ghost_id: &str,
    action: &str,
    fact: Option<&str>,
    find: Option<&str>,
    until: Option<&str>,
    topic: Option<&str>,
) -> Result<serde_json::Value, DomainStateError> {
    if let Some(until) = until {
        if !valid_date(until) {
            return Err(DomainStateError::bad_request(
                "--until must be a date like 2026-12-31.",
            ));
        }
    }
    let name = topic_file(topic);
    match action {
        "add" => {
            let line = fact_line(&today(), &clean_fact(fact.unwrap_or_default())?, until);
            let hash = edit_memory_file(paths, ghost_id, &name, |memory| {
                Ok(add_fact(memory, &line))
            })?;
            Ok(serde_json::json!({ "ok": true, "file": name, "hash": hash, "line": line }))
        }
        "replace" => {
            let line = fact_line(&today(), &clean_fact(fact.unwrap_or_default())?, until);
            let find = find.unwrap_or_default();
            let hash = edit_memory_file(paths, ghost_id, &name, |memory| {
                replace_fact(memory, find, &line)
            })?;
            Ok(serde_json::json!({ "ok": true, "file": name, "hash": hash, "line": line }))
        }
        "remove" => {
            let find = find.unwrap_or_default();
            let hash = edit_memory_file(paths, ghost_id, &name, |memory| remove_fact(memory, find))?;
            Ok(serde_json::json!({ "ok": true, "file": name, "hash": hash }))
        }
        "search" => {
            let query = find.or(fact).unwrap_or_default().trim().to_lowercase();
            let mut hits = Vec::new();
            for file in search_files(paths, ghost_id) {
                let text = read_ghost_file(paths, ghost_id, &file)?;
                for line in text.lines() {
                    if !line.trim().is_empty()
                        && (query.is_empty() || line.to_lowercase().contains(&query))
                    {
                        hits.push(serde_json::json!({ "file": file, "line": line }));
                    }
                }
            }
            Ok(serde_json::json!({ "matches": hits }))
        }
        _ => Err(DomainStateError::bad_request(
            "Memory actions are add, replace, remove and search.",
        )),
    }
}

/// MEMORY.md, the archive, then every topic note.
pub fn search_files(paths: &GxserverPaths, ghost_id: &str) -> Vec<String> {
    let mut files = vec![
        GHOST_MEMORY_FILE.to_string(),
        GHOST_MEMORY_ARCHIVE_FILE.to_string(),
    ];
    files.extend(list_topics(paths, ghost_id).into_iter().map(|topic| format!("memory/{topic}.md")));
    files
}

/// The topic notes under `memory/`, without the archive.
pub fn list_topics(paths: &GxserverPaths, ghost_id: &str) -> Vec<String> {
    let dir = super::files::ghost_home_dir(paths, ghost_id).join("memory");
    let mut topics = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".md").map(str::to_string)
        })
        .filter(|topic| topic != "archive")
        .collect::<Vec<_>>();
    topics.sort();
    topics
}

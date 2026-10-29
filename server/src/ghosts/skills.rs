//! A Ghost's own skills: skill folders that only this Ghost's agents see.
//!
//! CDXC:Ghosts 2026-10-29 WHY:
//! The skills live in the Ghost's home folder under `.agents/skills/<name>` (the real copy, which Codex, OpenCode and the other universal agents read from their working directory) with a link at `.claude/skills/<name>` (Claude Code's project skills). Every skill the user installs globally in `~/.agents/skills` is already visible to every agent, the Ghost included, so a Ghost skill is a copy the Ghost owns and can change without touching anyone else's.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::files::ghost_home_dir;
use crate::agent_skills::{copy_skill_directory_into_place, link_skill_into_agent_root};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

const SKILL_FILE: &str = "SKILL.md";

fn own_skills_dir(paths: &GxserverPaths, ghost_id: &str) -> PathBuf {
    ghost_home_dir(paths, ghost_id).join(".agents").join("skills")
}

fn claude_skills_dir(paths: &GxserverPaths, ghost_id: &str) -> PathBuf {
    ghost_home_dir(paths, ghost_id).join(".claude").join("skills")
}

fn global_skills_dir(paths: &GxserverPaths) -> PathBuf {
    paths
        .agent_config_home_dir()
        .join(crate::agent_skills::CANONICAL_AGENT_SKILLS_DIR)
}

fn validate_skill_name(name: &str) -> Result<(), DomainStateError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_');
    if ok {
        Ok(())
    } else {
        Err(DomainStateError::bad_request(
            "A skill name is lowercase letters, digits, - and _ (at most 64).",
        ))
    }
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Could not {action} {}: {error}", path.display()),
    }
}

/// The `description:` line of a SKILL.md front matter, if any.
fn skill_description(skill_dir: &Path) -> String {
    let Ok(text) = fs::read_to_string(skill_dir.join(SKILL_FILE)) else {
        return String::new();
    };
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    lines
        .take_while(|line| line.trim() != "---")
        .find_map(|line| line.strip_prefix("description:"))
        .map(|description| description.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

fn skill_rows(dir: &Path) -> Vec<Value> {
    let mut names = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join(SKILL_FILE).is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            json!({
                "name": name,
                "description": skill_description(&dir.join(&name)),
            })
        })
        .collect()
}

/// The Ghost's own skills, plus the global skills it could copy in.
pub fn list_ghost_skills(paths: &GxserverPaths, ghost_id: &str) -> Value {
    let own = skill_rows(&own_skills_dir(paths, ghost_id));
    let own_names = own
        .iter()
        .filter_map(|row| row["name"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let available = skill_rows(&global_skills_dir(paths))
        .into_iter()
        .filter(|row| {
            row["name"]
                .as_str()
                .is_some_and(|name| !own_names.iter().any(|own| own == name))
        })
        .collect::<Vec<_>>();
    json!({ "skills": own, "available": available })
}

/// Links an own skill into Claude Code's project skill folder.
fn link_for_claude(paths: &GxserverPaths, ghost_id: &str, name: &str) -> Result<(), DomainStateError> {
    link_skill_into_agent_root(
        &own_skills_dir(paths, ghost_id).join(name),
        &claude_skills_dir(paths, ghost_id).join(name),
    )?;
    Ok(())
}

/// Copies an installed global skill into the Ghost.
pub fn add_ghost_skill_from_global(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
) -> Result<(), DomainStateError> {
    validate_skill_name(name)?;
    let source = global_skills_dir(paths).join(name);
    if !source.join(SKILL_FILE).is_file() {
        return Err(DomainStateError::not_found(format!(
            "No installed skill is named {name}."
        )));
    }
    copy_skill_directory_into_place(&source, &own_skills_dir(paths, ghost_id).join(name))?;
    link_for_claude(paths, ghost_id, name)
}

/// Writes a new skill (or replaces one) from its SKILL.md text.
pub fn write_ghost_skill(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
    skill_md: &str,
) -> Result<(), DomainStateError> {
    validate_skill_name(name)?;
    if skill_md.trim().is_empty() || skill_md.len() > 64 * 1024 {
        return Err(DomainStateError::bad_request(
            "A skill's SKILL.md must be between 1 byte and 64 KB.",
        ));
    }
    let dir = own_skills_dir(paths, ghost_id).join(name);
    fs::create_dir_all(&dir).map_err(|error| io_error("create", &dir, error))?;
    let file = dir.join(SKILL_FILE);
    fs::write(&file, skill_md).map_err(|error| io_error("write", &file, error))?;
    link_for_claude(paths, ghost_id, name)
}

pub fn remove_ghost_skill(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
) -> Result<(), DomainStateError> {
    validate_skill_name(name)?;
    let link = claude_skills_dir(paths, ghost_id).join(name);
    if link.symlink_metadata().is_ok() {
        fs::remove_file(&link)
            .or_else(|_| fs::remove_dir_all(&link))
            .map_err(|error| io_error("remove", &link, error))?;
    }
    let dir = own_skills_dir(paths, ghost_id).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| io_error("remove", &dir, error))?;
    }
    Ok(())
}

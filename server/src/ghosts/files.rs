//! A Ghost's home folder: its Soul, its Memory, and the context files every agent reads from it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::records::GhostRecord;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// The error code of a write that named a hash the file no longer has.
pub const GHOST_FILE_CONFLICT: &str = "ghostFileConflict";
pub const GHOST_SOUL_FILE: &str = "SOUL.md";
pub const GHOST_MEMORY_FILE: &str = "MEMORY.md";
pub const GHOST_MEMORY_ARCHIVE_FILE: &str = "memory/archive.md";
/// A Soul or Memory file larger than this is refused.
pub const GHOST_FILE_MAX_BYTES: usize = 256 * 1024;
/// How much of MEMORY.md the context files carry; the rest stays readable on demand.
pub const GHOST_MEMORY_CONTEXT_MAX_LINES: usize = 200;
pub const GHOST_MEMORY_CONTEXT_MAX_BYTES: usize = 24 * 1024;

const DEFAULT_SOUL: &str = "# Soul\n\nWho this Ghost is: its personality, how it talks, what it cares about.\n\n## Standing instructions\n- Talk to the user the way they talk to you.\n";
const DEFAULT_MEMORY: &str = "# Memory\n\n";

/// CDXC:Ghosts 2026-10-29 WHY:
/// The home folder lives in Ghostex's own data folder, never inside a repository, so a pull request can never rewrite a Ghost's Soul or Memory (the reason Monocode keeps a Mono's files out of repos too). The Ghost's chat runs with this folder as its working directory, so the generated AGENTS.md and CLAUDE.md reach every agent on each start and resume without a per-agent launch flag.
pub fn ghost_home_dir(paths: &GxserverPaths, ghost_id: &str) -> PathBuf {
    paths.app_data_dir.join("ghosts").join(ghost_id)
}

pub fn content_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Ghost file error: {error}"),
    }
}

/// Only the Soul, the Memory and topic notes under `memory/` may be read or written by name.
pub fn validate_ghost_file_name(name: &str) -> Result<(), DomainStateError> {
    let topic = name
        .strip_prefix("memory/")
        .and_then(|rest| rest.strip_suffix(".md"));
    let topic_ok = topic.is_some_and(|topic| {
        !topic.is_empty()
            && topic.len() <= 64
            && topic
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    });
    if name == GHOST_SOUL_FILE || name == GHOST_MEMORY_FILE || topic_ok {
        Ok(())
    } else {
        Err(DomainStateError::bad_request(
            "A Ghost file is SOUL.md, MEMORY.md or memory/<topic>.md.",
        ))
    }
}

/// Makes the home folder and seeds a missing Soul and Memory.
pub fn ensure_ghost_home(paths: &GxserverPaths, ghost_id: &str) -> Result<PathBuf, DomainStateError> {
    let home = ghost_home_dir(paths, ghost_id);
    fs::create_dir_all(home.join("memory")).map_err(io_error)?;
    for (name, seed) in [(GHOST_SOUL_FILE, DEFAULT_SOUL), (GHOST_MEMORY_FILE, DEFAULT_MEMORY)] {
        let path = home.join(name);
        if !path.exists() {
            write_atomic(&path, seed)?;
        }
    }
    Ok(home)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), DomainStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    fs::write(&temp, text).map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        io_error(error)
    })
}

pub fn read_ghost_file(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
) -> Result<String, DomainStateError> {
    validate_ghost_file_name(name)?;
    match fs::read_to_string(ghost_home_dir(paths, ghost_id).join(name)) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(io_error(error)),
    }
}

pub fn ghost_file_value(text: &str) -> Value {
    json!({ "text": text, "hash": content_hash(text) })
}

/// Writes a Ghost file only when it still has the hash the writer read, so the user's edit in
/// Settings and the agent's own memory write never overwrite each other.
pub fn write_ghost_file(
    paths: &GxserverPaths,
    ghost_id: &str,
    name: &str,
    text: &str,
    expected_hash: Option<&str>,
) -> Result<String, DomainStateError> {
    validate_ghost_file_name(name)?;
    if text.len() > GHOST_FILE_MAX_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "{name} is limited to {} KB.",
            GHOST_FILE_MAX_BYTES / 1024
        )));
    }
    let current = read_ghost_file(paths, ghost_id, name)?;
    if let Some(expected) = expected_hash {
        if expected != content_hash(&current) {
            return Err(DomainStateError {
                code: GHOST_FILE_CONFLICT,
                message: format!("{name} changed since it was read. Read it again and retry."),
            });
        }
    }
    write_atomic(&ghost_home_dir(paths, ghost_id).join(name), text)?;
    Ok(content_hash(text))
}

/// The part of MEMORY.md that goes into the context files: the first lines up to the caps.
pub fn memory_for_context(memory: &str) -> String {
    let mut out = String::new();
    for (index, line) in memory.lines().enumerate() {
        if index >= GHOST_MEMORY_CONTEXT_MAX_LINES
            || out.len() + line.len() + 1 > GHOST_MEMORY_CONTEXT_MAX_BYTES
        {
            out.push_str("\n(More memory is in MEMORY.md; search it with `ghostex ghost memory search`.)\n");
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// One project a Ghost holds, as the context files name it.
pub struct GhostProjectBrief {
    pub project_id: String,
    pub name: String,
    pub path: String,
}

pub fn render_context_file(
    ghost: &GhostRecord,
    projects: &[GhostProjectBrief],
    soul: &str,
    memory: &str,
) -> String {
    let projects = if projects.is_empty() {
        "- (none yet: the user adds projects in this Ghost's settings)\n".to_string()
    } else {
        projects
            .iter()
            .map(|project| {
                format!(
                    "- {} — `{}` (project id `{}`)\n",
                    project.name, project.path, project.project_id
                )
            })
            .collect()
    };
    format!(
        "<!-- Written by Ghostex from this Ghost's Soul, Memory and projects. Edits here are replaced; change them in the Ghost's settings or with `ghostex ghost`. -->\n\n\
# You are {name}, a Ghost in Ghostex\n\n\
You are a long-lived agent the user talks to. You remember across conversations through your Memory, you look after the projects below, and you hand real work to threads you start in those projects. \
Run `ghostex ghost guide` once at the start of a conversation: it lists your tools (memory, threads, documents, habits) and how to use them.\n\n\
## Your projects\n\n{projects}\n\
## Soul\n\n{soul}\n\n\
## Memory\n\n{memory}",
        name = ghost.name,
        soul = soul.trim(),
        memory = memory_for_context(memory).trim_end(),
    )
}

/// Rewrites AGENTS.md and CLAUDE.md in the home folder from the current Soul, Memory and projects.
/// A running agent keeps what it started with; the next start or resume reads the new text.
pub fn regenerate_context_files(
    paths: &GxserverPaths,
    ghost: &GhostRecord,
    projects: &[GhostProjectBrief],
) -> Result<(), DomainStateError> {
    let home = ensure_ghost_home(paths, &ghost.ghost_id)?;
    let soul = read_ghost_file(paths, &ghost.ghost_id, GHOST_SOUL_FILE)?;
    let memory = read_ghost_file(paths, &ghost.ghost_id, GHOST_MEMORY_FILE)?;
    let text = render_context_file(ghost, projects, &soul, &memory);
    write_atomic(&home.join("AGENTS.md"), &text)?;
    write_atomic(&home.join("CLAUDE.md"), &text)?;
    Ok(())
}

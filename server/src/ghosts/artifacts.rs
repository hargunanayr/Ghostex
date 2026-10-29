//! A Ghost's documents: HTML pages and Markdown documents it keeps for the user, listed in its
//! Documents panel.
//!
//! CDXC:Ghosts 2026-10-29 WHY:
//! Each document is a file in the Ghost's home (`artifacts/<id>.html|.md`) with one line in `artifacts/index.json`, so the list survives without a table and the Ghost can read its own documents back. An HTML document is also published as a visual page (the `ghostex show` path, `visualPageId`), so it opens in the page window and the chat can show its card; a Markdown document opens as a file.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::files::ghost_home_dir;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

pub const GHOST_ARTIFACT_MAX_BYTES: usize = 8 * 1024 * 1024;
const ARTIFACT_TITLE_MAX_CHARS: usize = 200;
const ARTIFACT_SUMMARY_MAX_CHARS: usize = 400;

fn artifacts_dir(paths: &GxserverPaths, ghost_id: &str) -> PathBuf {
    ghost_home_dir(paths, ghost_id).join("artifacts")
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Ghost document error: {error}"),
    }
}

/// Every document, newest first.
pub fn read_artifact_index(paths: &GxserverPaths, ghost_id: &str) -> Vec<Value> {
    fs::read(artifacts_dir(paths, ghost_id).join("index.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
}

fn write_artifact_index(
    paths: &GxserverPaths,
    ghost_id: &str,
    index: &[Value],
) -> Result<(), DomainStateError> {
    let dir = artifacts_dir(paths, ghost_id);
    fs::create_dir_all(&dir).map_err(io_error)?;
    let temp = dir.join("index.json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(index).unwrap_or_default()).map_err(io_error)?;
    fs::rename(&temp, dir.join("index.json")).map_err(io_error)
}

fn clip(text: &str, max: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

pub struct ArtifactWrite<'a> {
    /// An existing document to replace; a new one gets a fresh id.
    pub id: Option<&'a str>,
    pub kind: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    pub text: &'a str,
    pub visual_page_id: Option<&'a str>,
    /// The page's full address as the publishing client reached it (`ghostex show`'s `url`).
    pub url: Option<&'a str>,
}

/// Stores a document and answers its index entry.
pub fn write_artifact(
    paths: &GxserverPaths,
    ghost_id: &str,
    write: &ArtifactWrite<'_>,
) -> Result<Value, DomainStateError> {
    let extension = match write.kind {
        "html" => "html",
        "markdown" => "md",
        _ => {
            return Err(DomainStateError::bad_request(
                "A document's kind is html or markdown.",
            ))
        }
    };
    let title = clip(write.title, ARTIFACT_TITLE_MAX_CHARS);
    if title.is_empty() {
        return Err(DomainStateError::bad_request("A document needs a title."));
    }
    if write.text.trim().is_empty() || write.text.len() > GHOST_ARTIFACT_MAX_BYTES {
        return Err(DomainStateError::bad_request(
            "A document must be between 1 byte and 8 MiB.",
        ));
    }
    let mut index = read_artifact_index(paths, ghost_id);
    let existing = write
        .id
        .and_then(|id| index.iter().position(|entry| entry["id"].as_str() == Some(id)));
    if write.id.is_some() && existing.is_none() {
        return Err(DomainStateError::not_found("No document has that id."));
    }
    let id = write
        .id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..12].to_string());
    let dir = artifacts_dir(paths, ghost_id);
    fs::create_dir_all(&dir).map_err(io_error)?;
    let file = dir.join(format!("{id}.{extension}"));
    fs::write(&file, write.text).map_err(io_error)?;
    let now = crate::coordinators::now_iso();
    let mut entry = json!({
        "id": id,
        "kind": write.kind,
        "title": title,
        "summary": clip(write.summary, ARTIFACT_SUMMARY_MAX_CHARS),
        "file": file.to_string_lossy(),
        "createdAt": now,
        "updatedAt": now,
    });
    if let Some(page) = write.visual_page_id.filter(|page| !page.is_empty()) {
        entry["visualPageId"] = json!(page);
        entry["url"] = json!(write
            .url
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .map(str::to_string)
            .unwrap_or_else(|| format!(
                "{}{page}",
                crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX
            )));
    }
    if let Some(position) = existing {
        entry["createdAt"] = index[position]["createdAt"].clone();
        index.remove(position);
    }
    index.insert(0, entry.clone());
    write_artifact_index(paths, ghost_id, &index)?;
    Ok(entry)
}

pub fn remove_artifact(
    paths: &GxserverPaths,
    ghost_id: &str,
    id: &str,
) -> Result<(), DomainStateError> {
    let mut index = read_artifact_index(paths, ghost_id);
    let Some(position) = index.iter().position(|entry| entry["id"].as_str() == Some(id)) else {
        return Err(DomainStateError::not_found("No document has that id."));
    };
    let entry = index.remove(position);
    if let Some(file) = entry["file"].as_str() {
        let file = PathBuf::from(file);
        if file.starts_with(artifacts_dir(paths, ghost_id)) {
            let _ = fs::remove_file(file);
        }
    }
    write_artifact_index(paths, ghost_id, &index)
}

/// One document with its text.
pub fn read_artifact(
    paths: &GxserverPaths,
    ghost_id: &str,
    id: &str,
) -> Result<Value, DomainStateError> {
    let entry = read_artifact_index(paths, ghost_id)
        .into_iter()
        .find(|entry| entry["id"].as_str() == Some(id))
        .ok_or_else(|| DomainStateError::not_found("No document has that id."))?;
    let text = entry["file"]
        .as_str()
        .and_then(|file| fs::read_to_string(file).ok())
        .unwrap_or_default();
    let mut entry = entry;
    entry["text"] = json!(text);
    Ok(entry)
}

//! A finished turn's changed files as a list: one row per file, with the folder it sits in, and a
//! file written more than once in the turn is one row that remembers every write.
//!
//! CDXC:SessionChat 2026-10-27 DECISION: User (from a reference screenshot): when a turn changed files, show them at the end of the turn in a card with "N files changed", the total +/− and an Open diff button that opens the Commit dialog. The card starts collapsed to its header ("Defaultnya collapse dong biar gak numpuk"), and it opens to a plain list of the files ("isi dari Edited N files diubah menjadi list saja"), superseding the folder tree tried the same day. The GPUI chat draws it in `native_chat/file_change_list.rs`.
//! SEE-ALSO: apps/desktop/src/app/native_chat/file_change_list.rs,
//! apps/mobile/app/src/chat/native/transcript/FileChanges.tsx.

use serde_json::{json, Value};

struct Row {
    name: String,
    folder: String,
    /// The display path the rows are merged and ordered by.
    key: String,
    path: String,
    status: String,
    added: u64,
    removed: u64,
    failed: bool,
    /// The rows of the turn's `files` this row stands for, in the order they were written.
    entries: Vec<usize>,
}

/// The display path's folders and file name. An absolute path keeps its root on the first folder
/// so it does not read as a project folder.
fn segments(display: &str) -> Vec<String> {
    let mut parts: Vec<String> = display
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    if display.starts_with('/') {
        if let Some(first) = parts.first_mut() {
            first.insert(0, '/');
        }
    }
    parts
}

fn status_letter(status: &str) -> &'static str {
    match status {
        "new" => "A",
        "deleted" => "D",
        _ => "M",
    }
}

/// A shared folder for the header, with the home folder written `~` as the file rows write it.
fn home_relative(parts: &[String]) -> String {
    match parts {
        [top, _user, rest @ ..] if top == "/Users" || top == "/home" => std::iter::once("~")
            .chain(rest.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("/"),
        _ => parts.join("/"),
    }
}

/// Whether a display path is outside the project: absolute, under the home folder, or on a drive.
fn is_outside(display: &str) -> bool {
    let bytes = display.as_bytes();
    display.starts_with(['/', '\\', '~'])
        || (bytes.first().is_some_and(u8::is_ascii_alphabetic) && bytes.get(1) == Some(&b':'))
}

/// The list over a turn's file rows (`transcript_rows::file_rows`), ordered by path so a folder's
/// files sit together, and the folder the rows' folders are written from. Each row is
/// `{name, folder, key, path, status, letter, added, removed, failed, entries}`, where `entries`
/// index into the same `files` the card draws its diffs from.
///
/// Project paths keep their project folders, as the reference did. When every path is outside the
/// project (the session's folder was not known, or the turn wrote elsewhere) the rows' folders
/// start at the folder they all share instead of at `/Users`, and that folder is returned to be
/// named in the header.
pub fn file_change_list(files: &[Value]) -> (Vec<Value>, String) {
    let split: Vec<(usize, &Value, &str, Vec<String>)> = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let display = file["displayPath"]
                .as_str()
                .filter(|display| !display.is_empty())
                .or_else(|| file["path"].as_str())?;
            let parts = segments(display);
            (!parts.is_empty()).then_some((index, file, display, parts))
        })
        .collect();
    let mut shared = 0;
    if split.iter().all(|(_, _, display, _)| is_outside(display)) {
        let folders = |parts: &Vec<String>| parts.len().saturating_sub(1);
        if let Some((_, _, _, first)) = split.first() {
            shared = folders(first);
            for (_, _, _, parts) in &split[1..] {
                shared = first
                    .iter()
                    .zip(parts.iter())
                    .take(shared.min(folders(parts)))
                    .take_while(|(left, right)| left == right)
                    .count();
            }
        }
    }
    let root_label = split
        .first()
        .map(|(_, _, _, parts)| home_relative(&parts[..shared]))
        .unwrap_or_default();
    let mut rows: Vec<Row> = Vec::new();
    for (index, file, display, parts) in &split {
        let added = file["added"].as_u64().unwrap_or(0);
        let removed = file["removed"].as_u64().unwrap_or(0);
        let status = file["status"].as_str().unwrap_or("edited");
        let failed = file["failed"] == true;
        if let Some(row) = rows.iter_mut().find(|row| row.key == *display) {
            row.added += added;
            row.removed += removed;
            // A file the turn created stays new; one it removed last reads as removed.
            if status == "deleted" {
                row.status = "deleted".to_string();
            }
            row.failed &= failed;
            row.entries.push(*index);
            continue;
        }
        let (name, folders) = parts.split_last().expect("segments are non-empty");
        rows.push(Row {
            name: name.clone(),
            folder: folders[shared..].join("/"),
            key: display.to_string(),
            path: file["path"].as_str().unwrap_or_default().to_string(),
            status: status.to_string(),
            added,
            removed,
            failed,
            entries: vec![*index],
        });
    }
    rows.sort_by_key(|row| row.key.to_lowercase());
    let rows = rows
        .into_iter()
        .map(|row| {
            json!({
                "name": row.name,
                "folder": row.folder,
                "key": format!("file:{}", row.key),
                "path": row.path,
                "status": row.status,
                "letter": status_letter(&row.status),
                "added": row.added,
                "removed": row.removed,
                "failed": row.failed,
                "entries": row.entries,
            })
        })
        .collect();
    (rows, root_label)
}

/// The turn card's header: the message card's summary with the list, and the folder the rows'
/// folders start from when that is not the project's (`listRoot`, empty otherwise).
pub fn file_change_turn_card(files: &[Value]) -> Value {
    let mut card = crate::transcript::file_change_rows::file_change_card(files);
    let (list, root) = file_change_list(files);
    if let Value::Object(fields) = &mut card {
        fields.insert("list".to_string(), Value::Array(list));
        fields.insert("listRoot".to_string(), Value::String(root));
    }
    card
}

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{
    request_gxserver_rpc, resolve_gxserver_server_target, CliError, CliResult,
};
use crate::visual_pages::{
    page_too_large, prepare_page, PUBLISH_VISUAL_PAGE_ENDPOINT, VISUAL_PAGE_MAX_BYTES,
};

/// `ghostex show <file.html> [--title <text>] [--json]`: publishes a self-contained HTML page
/// through gxserver and prints the ```visual block that shows it as a card in the chat.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: "both": the chat's native ```visual blocks (charts, stats, tables, text) and HTML pages ship together. A page covers what a block cannot express, such as a UI mockup or a small interactive tool; this verb publishes it and prints the page block the agent pastes into its reply.
pub fn show_command(args: &[String]) -> CliResult<()> {
    if matches!(args.first().map(String::as_str), Some("help")) {
        println!("{}", crate::ghostex_cli::usage::command_usage("show"));
        return Ok(());
    }
    let parsed = parse_args(args);
    // `--browser` takes no value, but the shared parser reads the next word as one when the file
    // comes after it.
    let browser_value = parsed
        .flags
        .string_value("browser")
        .filter(|value| !matches!(*value, "true" | "false"))
        .map(str::to_string);
    let file = parsed.rest.first().cloned().or(browser_value);
    let popup = !parsed.flags.contains("browser");
    let Some(file) = file.as_ref() else {
        return Err(CliError::Other(
            "show needs an HTML file: ghostex show <file.html> [--title <text>] [--browser] [--json]"
                .to_string(),
        ));
    };
    let published = publish_html_file(
        &absolute_path(file),
        parsed.flags.string_value("title"),
        popup,
        &parsed.flags,
    )?;
    let (id, title, url, block) = (
        published.id.as_str(),
        published.title.as_str(),
        published.url.as_str(),
        published.block.as_str(),
    );
    if parsed.flags.truthy("json") {
        println!(
            "{{\"ok\":true,\"id\":{},\"title\":{},\"url\":{},\"block\":{}}}",
            json_string(id),
            json_string(title),
            json_string(url),
            json_string(block)
        );
    } else {
        println!(
            "Published \"{title}\".\nPut this block in your reply where the page should appear (the chat shows it as a card that {}):\n\n{block}",
            if popup {
                "opens the page in a floating window over the chat"
            } else {
                "opens the page in the browser"
            }
        );
    }
    Ok(())
}

/// A page `publish_html_file` published: its id, title, address, and the ```visual block that
/// shows it as a card in the chat.
pub(crate) struct PublishedPage {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) block: String,
    /// The file's own HTML, before gxserver's head and images were added.
    pub(crate) source: String,
}

/// Prepares an HTML file and publishes it through gxserver, as `ghostex show` does.
pub(crate) fn publish_html_file(
    path: &Path,
    title: Option<&str>,
    popup: bool,
    flags: &crate::ghostex_cli::args::Flags,
) -> CliResult<PublishedPage> {
    let source = read_source(path)?;
    let base_dir = path.parent().unwrap_or(Path::new("/"));
    let prepared = prepare_page(&source, base_dir).map_err(CliError::Other)?;
    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or(prepared.title)
        .unwrap_or_else(|| file_stem(path));
    // gxserver keeps the file's path so the desktop's page window can offer to refresh the page
    // when the file changes (visual_pages/source.rs).
    let mut params = json!({
        "title": title,
        "html": prepared.html,
        "sourcePath": path.to_string_lossy(),
    });
    if let Some(session_ref) = calling_session_ref() {
        params["sessionRef"] = Value::String(session_ref);
    }
    let target = resolve_gxserver_server_target(flags, &params)?;
    let result = request_gxserver_rpc(&target, PUBLISH_VISUAL_PAGE_ENDPOINT, &params, flags)?;
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or_default();
    let (id, page_path) = (text("id"), text("path"));
    if id.is_empty() || page_path.is_empty() {
        return Err(CliError::Other(
            "gxserver did not return the published page's address. Update Ghostex and try again."
                .to_string(),
        ));
    }
    let title = Some(text("title"))
        .filter(|title| !title.is_empty())
        .unwrap_or(title.as_str())
        .to_string();
    let url = format!("{}{page_path}", target.base_url.trim_end_matches('/'));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let block = visual_page_block(
        &title,
        &url,
        &file_name,
        popup,
        page_question_tags(&source).as_deref(),
    );
    Ok(PublishedPage {
        id: id.to_string(),
        title,
        url,
        block,
        source,
    })
}

/// The tags of the questions a page asks (`<gx-question tag="Q1">`), or `None` when it asks none:
/// the page holds the question kit (`<gx-questions>`) or sends answers of its own
/// (`ghostex.submit`).
///
/// CDXC:SessionChat 2026-10-14 WHY:
/// The chat counts a question page as unanswered until its answers come back, and links its tags, but the card only knows what the block says; the page's HTML is not in the transcript. So the block names the tags it asks (`"questions": [...]`), read here from the file being published.
fn page_question_tags(source: &str) -> Option<Vec<String>> {
    if !source.contains("<gx-questions") && !source.contains("ghostex.submit(") {
        return None;
    }
    let mut tags: Vec<String> = Vec::new();
    for (at, _) in source.match_indices("<gx-question") {
        let rest = &source[at + "<gx-question".len()..];
        if !rest.starts_with(|character: char| character.is_whitespace() || character == '>') {
            continue;
        }
        let head = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(tag) = attribute(head, "tag").filter(|tag| !tag.is_empty()) {
            if !tags.iter().any(|known| known.eq_ignore_ascii_case(&tag)) {
                tags.push(tag);
            }
        }
    }
    Some(tags)
}

/// An attribute's value in a start tag's text (`tag="Q1"` or `tag='Q1'`).
fn attribute(head: &str, name: &str) -> Option<String> {
    let mut search = head;
    while let Some(at) = search.find(name) {
        let before = search[..at].chars().next_back();
        let after = search[at + name.len()..].trim_start();
        search = &search[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].trim().to_string());
    }
    None
}

/// CDXC:SessionChat 2026-10-06 SEE-ALSO:
/// The page block `{"page":{"title","url","file","open","questions"}}` is a contract with the chat's visual parser in packages/gx-visual, which draws it as a card whose Open button opens the URL, and with skills/ghostex-visuals/SKILL.md, which tells agents to paste it exactly as printed.
/// CDXC:SessionChat 2026-10-09 DECISION:
/// User: the agent gives its HTML link "some mark so when we click to open it it's intended to be opened floating", so the agent can show a card for its page in the chat. The mark is `"open": "popup"`, printed by default; `--browser` leaves it out so the card opens the page in the browser instead.
fn visual_page_block(
    title: &str,
    url: &str,
    file: &str,
    popup: bool,
    questions: Option<&[String]>,
) -> String {
    let open = if popup { ",\"open\":\"popup\"" } else { "" };
    let questions = questions
        .map(|tags| format!(",\"questions\":{}", json!(tags)))
        .unwrap_or_default();
    format!(
        "```visual\n{{\"page\":{{\"title\":{},\"url\":{},\"file\":{}{open}{questions}}}}}\n```",
        json_string(title),
        json_string(url),
        json_string(file)
    )
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

pub(crate) fn absolute_path(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

fn read_source(path: &Path) -> CliResult<String> {
    let metadata = fs::metadata(path)
        .map_err(|_| CliError::Other(format!("Could not find {}.", path.display())))?;
    if !metadata.is_file() {
        return Err(CliError::Other(format!(
            "{} is not a file.",
            path.display()
        )));
    }
    // The page only grows once its head and images are added, so a source already over the
    // finished page's limit is refused before it is read.
    if metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return Err(CliError::Other(page_too_large(
            usize::try_from(metadata.len()).unwrap_or(usize::MAX),
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| CliError::Other(format!("Could not read {}: {error}", path.display())))?;
    String::from_utf8(bytes).map_err(|_| {
        CliError::Other(
            "The page is not UTF-8 text. Save it as UTF-8 and run ghostex show again.".to_string(),
        )
    })
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "Page".to_string())
}

/// The session this runs in, from the environment every Ghostex pane exports; kept with the page
/// so it can be traced back to the conversation that made it, and so the reader's answers
/// (`ghostex.submit`) reach that session.
fn calling_session_ref() -> Option<String> {
    ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_NATIVE_SESSION_ID"]
        .iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

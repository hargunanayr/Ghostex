use serde_json::json;

use super::command::{flag_text, flags, target_ghost};
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    show::{absolute_path, publish_html_file},
};

/// `ghostex ghost artifact write|list|read|remove`.
pub(super) fn run(parsed: &ParsedArgs) -> CliResult<()> {
    let ghost = target_ghost(parsed)?;
    let flags = flags(parsed);
    let action = parsed.rest.first().map(String::as_str).unwrap_or("list");
    match action {
        "write" => write(parsed, &ghost),
        "list" => {
            let listed = call_gxserver_rpc(
                "/api/ghostArtifacts",
                &json!({ "ghostId": ghost["ghostId"], "action": "list" }),
                &flags,
            )?;
            if parsed.flags.truthy("json") {
                print_json(&listed);
                return Ok(());
            }
            let artifacts = listed["artifacts"].as_array().cloned().unwrap_or_default();
            if artifacts.is_empty() {
                println!("No documents yet.");
            }
            for artifact in artifacts {
                println!(
                    "{}  {:<8}  {}  {}",
                    agents::text(&artifact, "id"),
                    agents::text(&artifact, "kind"),
                    agents::text(&artifact, "title"),
                    agents::text(&artifact, "updatedAt"),
                );
            }
            Ok(())
        }
        "read" | "remove" => {
            let id = parsed.rest.get(1).cloned().ok_or_else(|| {
                CliError::Other(format!("artifact {action} needs a document id from artifact list."))
            })?;
            let result = call_gxserver_rpc(
                "/api/ghostArtifacts",
                &json!({ "ghostId": ghost["ghostId"], "action": action, "id": id }),
                &flags,
            )?;
            if action == "read" && !parsed.flags.truthy("json") {
                print!("{}", agents::text(&result["artifact"], "text"));
            } else {
                print_json(&result);
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown artifact command: {other}. Use write, list, read or remove."
        ))),
    }
}

fn write(parsed: &ParsedArgs, ghost: &serde_json::Value) -> CliResult<()> {
    let flags = flags(parsed);
    let file = flag_text(parsed, "file")
        .ok_or_else(|| CliError::Other("artifact write needs --file <path>.".into()))?;
    let path = absolute_path(&file);
    let kind = flag_text(parsed, "kind").unwrap_or_else(|| {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("html" | "htm") => "html".to_string(),
            _ => "markdown".to_string(),
        }
    });
    let title = flag_text(parsed, "title");
    let (text, title, page, block, url) = match kind.as_str() {
        "html" => {
            let page = publish_html_file(&path, title.as_deref(), true, &flags)?;
            (page.source, page.title, Some(page.id), Some(page.block), Some(page.url))
        }
        "markdown" => {
            let text = std::fs::read_to_string(&path).map_err(|error| {
                CliError::Other(format!("Could not read {}: {error}", path.display()))
            })?;
            let title = title.ok_or_else(|| {
                CliError::Other("A Markdown document needs --title <title>.".into())
            })?;
            (text, title, None, None, None)
        }
        _ => {
            return Err(CliError::Other(
                "--kind is html or markdown.".into(),
            ))
        }
    };
    let stored = call_gxserver_rpc(
        "/api/ghostArtifacts",
        &json!({
            "ghostId": ghost["ghostId"],
            "action": "write",
            "id": flag_text(parsed, "id"),
            "kind": kind,
            "title": title,
            "summary": flag_text(parsed, "summary").unwrap_or_default(),
            "text": text,
            "visualPageId": page,
            "url": url,
        }),
        &flags,
    )?;
    let artifact = &stored["artifact"];
    if parsed.flags.truthy("json") {
        let mut result = stored.clone();
        if let Some(block) = &block {
            result["block"] = json!(block);
        }
        print_json(&result);
        return Ok(());
    }
    println!(
        "Saved \"{}\" to your Documents ({}).",
        agents::text(artifact, "title"),
        agents::text(artifact, "id")
    );
    if let Some(block) = block {
        println!(
            "Put this block in your reply where the document should appear (the chat shows it as a card that opens it):\n\n{block}"
        );
    }
    Ok(())
}

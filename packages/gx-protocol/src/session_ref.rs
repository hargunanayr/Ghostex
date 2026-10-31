//! A session's Global Ref (`<server>:<project>:<session>`, e.g. `S90:P3lv0:G7hgi`), the id every
//! `ghostex` verb takes, and the Markdown mention that names a session by it.
//!
//! CDXC:Cli 2026-10-28 DECISION:
//! User: "i want a new Copy ID that let's me just copy the id and name of an agent so i can mention it in other chats", written `[<session title>](<Global Ref>)` (the "·" in their template was a typo), and "we need to be able to do + to mention agents in the same project". So a mention is a plain Markdown link whose destination is the Global Ref: an agent that reads it can pass the ref to `ghostex agents send`, and the chats draw it as the session's agent pill.
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/copy.rs (Copy ID), packages/gx-chat-core/src/transcript/session_mentions.rs (the pill and the `+` picker), apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts (the phone's Copy ID), and `is_gxserver_global_session_ref` in server/src/ghostex_cli/rpc.rs, which accepts the same shape.

/// The three parts of a Global Ref.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSessionRef<'a> {
    pub server_id: &'a str,
    pub project_id: &'a str,
    pub session_id: &'a str,
}

/// `S…:P…:G…`, each part its letter, a digit, then lowercase letters and digits; `None` for
/// anything else, so a URL (`https://…`), a path or a `host:port` never reads as a session.
pub fn parse_global_session_ref(value: &str) -> Option<GlobalSessionRef<'_>> {
    let mut parts = value.split(':');
    let (Some(server_id), Some(project_id), Some(session_id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    fn segment_ok(segment: &str, prefix: char) -> bool {
        let mut chars = segment.chars();
        chars.next() == Some(prefix)
            && matches!(chars.next(), Some(c) if c.is_ascii_digit())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }
    (segment_ok(server_id, 'S') && segment_ok(project_id, 'P') && segment_ok(session_id, 'G'))
        .then_some(GlobalSessionRef {
            server_id,
            project_id,
            session_id,
        })
}

/// `[<title>](<global ref>)`: the title on one line, with the brackets and backslashes Markdown
/// would read escaped; "Session" when it has no title.
pub fn session_mention(title: &str, global_ref: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if title.is_empty() { "Session" } else { &title };
    let mut label = String::with_capacity(title.len());
    for character in title.chars() {
        if matches!(character, '\\' | '[' | ']') {
            label.push('\\');
        }
        label.push(character);
    }
    format!("[{label}]({global_ref})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_refs_are_three_lettered_parts() {
        let parsed = parse_global_session_ref("S90:P3lv0:G7hgi").expect("a ref");
        assert_eq!(
            (parsed.server_id, parsed.project_id, parsed.session_id),
            ("S90", "P3lv0", "G7hgi")
        );
        for other in [
            "https://example.com",
            "S90:P3lv0",
            "S90:P3lv0:G7hgi:extra",
            "S90:P3lv0:Gabc",
            "s90:P3lv0:G7hgi",
            "S90:P3LV0:G7hgi",
            "localhost:8080",
            "/tmp/S90:P3lv0:G7hgi",
            "",
        ] {
            assert!(parse_global_session_ref(other).is_none(), "{other}");
        }
    }

    #[test]
    fn a_mention_is_a_markdown_link_to_the_ref() {
        assert_eq!(
            session_mention(
                "Model and effort inventory for agent CLIs",
                "S90:P3lv0:G7hgi"
            ),
            "[Model and effort inventory for agent CLIs](S90:P3lv0:G7hgi)"
        );
        assert_eq!(
            session_mention("Fix [beta]\n  build", "S1:P2:G3"),
            r"[Fix \[beta\] build](S1:P2:G3)"
        );
        assert_eq!(session_mention("  ", "S1:P2:G3"), "[Session](S1:P2:G3)");
    }
}

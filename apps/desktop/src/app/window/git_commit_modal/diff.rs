//! The patch model the diff panel draws (`packages/core-ui/git-file-diff-modal.tsx` (deleted 2026-10-01)): patch lines
//! classified by kind, the display options, and the token kinds of the per-row syntax colouring.
use super::model::{ChangeEntry, ChangeSide, GitFileDiffDraft, sum_stats};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Addition,
    Context,
    Deletion,
    Hunk,
    Metadata,
    Raw,
}

impl DiffLineKind {
    /// The kinds whose rows carry code (and so syntax colouring and a split view).
    pub(crate) fn is_code(self) -> bool {
        matches!(self, Self::Addition | Self::Context | Self::Deletion)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub(crate) content: String,
    pub(crate) kind: DiffLineKind,
    /// 1-based position in the patch, the row's gutter number.
    pub(crate) number: usize,
}

pub(crate) const NO_DIFF_AVAILABLE: &str = "No diff is available for this file.";

/// `classifyDiffLine`.
fn classify_diff_line(line: &str) -> DiffLineKind {
    if line.starts_with("@@") {
        return DiffLineKind::Hunk;
    }
    const METADATA: [&str; 9] = [
        "diff --git",
        "index ",
        "new file mode",
        "deleted file mode",
        "similarity index",
        "rename from ",
        "rename to ",
        "--- ",
        "+++ ",
    ];
    if METADATA.iter().any(|prefix| line.starts_with(prefix)) {
        return DiffLineKind::Metadata;
    }
    if line.starts_with('+') {
        return DiffLineKind::Addition;
    }
    if line.starts_with('-') {
        return DiffLineKind::Deletion;
    }
    if line.starts_with("No diff is available") {
        return DiffLineKind::Raw;
    }
    DiffLineKind::Context
}

/// `parseDiffLines`: the patch without its trailing whitespace, one row per line, or the single
/// "No diff is available" row for an empty patch.
pub(crate) fn parse_diff_lines(patch: &str) -> Vec<DiffLine> {
    let raw: Vec<&str> = patch.trim_end().split('\n').collect();
    let lines: Vec<&str> = if raw.iter().any(|line| !line.is_empty()) {
        raw
    } else {
        vec![NO_DIFF_AVAILABLE]
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            // A CRLF patch keeps its `\r` in the browser, where it draws as nothing.
            let content = line.strip_suffix('\r').unwrap_or(line).to_string();
            DiffLine {
                kind: classify_diff_line(&content),
                content,
                number: index + 1,
            }
        })
        .collect()
}

/// `isWhitespaceOnlyChangeLine`.
pub(crate) fn is_whitespace_only_change(line: &DiffLine) -> bool {
    matches!(line.kind, DiffLineKind::Addition | DiffLineKind::Deletion)
        && line
            .content
            .get(1..)
            .map(str::trim)
            .is_none_or(str::is_empty)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GitDiffViewMode {
    #[default]
    Unified,
    Split,
}

/// `GitCommitDiffPreferences`: the diff display options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GitDiffPrefs {
    pub(crate) hide_whitespace: bool,
    pub(crate) line_wrap: bool,
    pub(crate) view_mode: GitDiffViewMode,
}

impl GitDiffPrefs {
    /// Side by side forces wrapping (`effectiveLineWrap`).
    pub(crate) fn effective_line_wrap(&self) -> bool {
        self.view_mode == GitDiffViewMode::Split || self.line_wrap
    }

    /// `readGitCommitDiffPreferences`: anything unreadable is the default.
    pub(crate) fn from_storage(raw: Option<&str>) -> Self {
        let Some(value) = raw.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        else {
            return Self::default();
        };
        Self {
            hide_whitespace: value.get("hideWhitespace").and_then(|v| v.as_bool()) == Some(true),
            line_wrap: value.get("lineWrap").and_then(|v| v.as_bool()) == Some(true),
            view_mode: if value.get("viewMode").and_then(|v| v.as_str()) == Some("split") {
                GitDiffViewMode::Split
            } else {
                GitDiffViewMode::Unified
            },
        }
    }

    /// `writeGitCommitDiffPreferences`'s JSON.
    pub(crate) fn to_storage(&self) -> String {
        serde_json::json!({
            "hideWhitespace": self.hide_whitespace,
            "lineWrap": self.line_wrap,
            "viewMode": match self.view_mode {
                GitDiffViewMode::Unified => "unified",
                GitDiffViewMode::Split => "split",
            },
        })
        .to_string()
    }
}

/// `buildLoadingFileDiffPatch`: the stand-in patch Show All draws for a file whose diff has not arrived.
fn loading_file_patch(file_path: &str) -> String {
    format!(
        "diff --git a/{file_path} b/{file_path}\n--- a/{file_path}\n+++ b/{file_path}\n@@ loading diff @@\n Loading diff..."
    )
}

/// `buildAllFilesDiffDraft`: every row's patch, one after another (the staged ones first).
pub(crate) fn all_files_diff_draft(
    entries: &[ChangeEntry],
    cache: &HashMap<(ChangeSide, String), GitFileDiffDraft>,
) -> Option<GitFileDiffDraft> {
    if entries.is_empty() {
        return None;
    }
    let stats = sum_stats(entries.iter().map(|entry| entry.stat));
    let patch = entries
        .iter()
        .map(|entry| {
            cache
                .get(&(entry.side, entry.path.clone()))
                .map(|draft| draft.patch.trim_end().to_string())
                .filter(|patch| !patch.is_empty())
                .unwrap_or_else(|| loading_file_patch(&entry.path))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(GitFileDiffDraft {
        file_path: "All files".to_string(),
        patch,
        additions: Some(stats.additions),
        deletions: Some(stats.deletions),
        staged: None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffTokenKind {
    Attribute,
    Comment,
    Function,
    Keyword,
    Literal,
    Marker,
    Number,
    Operator,
    Plain,
    String,
    Type,
}

const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "defer",
    "do",
    "else",
    "enum",
    "export",
    "extension",
    "final",
    "for",
    "from",
    "func",
    "function",
    "guard",
    "if",
    "implements",
    "import",
    "in",
    "interface",
    "let",
    "mut",
    "new",
    "override",
    "package",
    "private",
    "protected",
    "protocol",
    "public",
    "return",
    "static",
    "struct",
    "super",
    "switch",
    "throws",
    "throw",
    "try",
    "type",
    "var",
    "where",
    "while",
];

const LITERALS: &[&str] = &[
    "false",
    "nil",
    "null",
    "None",
    "self",
    "Self",
    "this",
    "true",
    "undefined",
];

const PRIMITIVE_TYPES: &[&str] = &[
    "Array",
    "Bool",
    "Boolean",
    "Date",
    "Dictionary",
    "Double",
    "Float",
    "Int",
    "Map",
    "Number",
    "Object",
    "Set",
    "String",
    "Void",
];

fn is_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
}

fn is_identifier_part(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn is_operator(ch: char) -> bool {
    "{}[]().,:;?<>+=*/%!&|^~-".contains(ch)
}

/// `tokenizeDiffLineContent`: the leading `+`, `-` or space as a marker, then the code's tokens,
/// as byte ranges of `content` (tokens never overlap and cover it in order).
///
/// CDXC:Git 2026-06-08-04:20:
/// Commit review diffs need visible syntax coloring in both the inline commit diff and standalone file diff without replacing the existing split/unified row model. Tokenize each rendered patch row so the diff layout, wrapping, and line-number behavior stay shared across both surfaces.
pub(crate) fn tokenize_diff_line(content: &str) -> Vec<(std::ops::Range<usize>, DiffTokenKind)> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    if content.starts_with(['+', '-', ' ']) {
        tokens.push((0..1, DiffTokenKind::Marker));
        offset = 1;
    }
    let code: Vec<(usize, char)> = content[offset..]
        .char_indices()
        .map(|(index, ch)| (index + offset, ch))
        .collect();
    let end = content.len();
    let at = |index: usize| code.get(index).map(|(_, ch)| *ch);
    let byte = |index: usize| code.get(index).map(|(byte, _)| *byte).unwrap_or(end);
    let mut index = 0;
    while index < code.len() {
        let ch = code[index].1;
        let start = byte(index);
        if is_space(ch) {
            while at(index).is_some_and(is_space) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Plain));
            continue;
        }
        if ch == '/' && at(index + 1) == Some('/') {
            tokens.push((start..end, DiffTokenKind::Comment));
            break;
        }
        if ch == '/' && at(index + 1) == Some('*') {
            let mut cursor = index + 2;
            while cursor < code.len() && !(at(cursor) == Some('*') && at(cursor + 1) == Some('/')) {
                cursor += 1;
            }
            let cursor = (cursor + 2).min(code.len());
            tokens.push((start..byte(cursor), DiffTokenKind::Comment));
            index = cursor;
            continue;
        }
        if ch == '#' && (index == 0 || at(index - 1).is_some_and(is_space)) {
            tokens.push((start..end, DiffTokenKind::Comment));
            break;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            let mut cursor = index + 1;
            let mut escaped = false;
            while cursor < code.len() {
                let next = code[cursor].1;
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == ch {
                    cursor += 1;
                    break;
                }
                cursor += 1;
            }
            tokens.push((start..byte(cursor), DiffTokenKind::String));
            index = cursor;
            continue;
        }
        if ch == '@' && at(index + 1).is_some_and(is_identifier_start) {
            let mut cursor = index + 1;
            while at(cursor).is_some_and(is_identifier_part) {
                cursor += 1;
            }
            tokens.push((start..byte(cursor), DiffTokenKind::Attribute));
            index = cursor;
            continue;
        }
        if ch.is_ascii_digit() {
            while at(index).is_some_and(|c| c.is_ascii_hexdigit() || "._xXoObB".contains(c)) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Number));
            continue;
        }
        if is_identifier_start(ch) {
            let mut cursor = index;
            while at(cursor).is_some_and(is_identifier_part) {
                cursor += 1;
            }
            let word = &content[start..byte(cursor)];
            let next = (cursor..code.len())
                .map(|i| code[i].1)
                .find(|c| !is_space(*c));
            let kind = if KEYWORDS.contains(&word) {
                DiffTokenKind::Keyword
            } else if LITERALS.contains(&word) {
                DiffTokenKind::Literal
            } else if PRIMITIVE_TYPES.contains(&word)
                || word.starts_with(|c: char| c.is_ascii_uppercase())
            {
                DiffTokenKind::Type
            } else if next == Some('(') {
                DiffTokenKind::Function
            } else {
                DiffTokenKind::Plain
            };
            tokens.push((start..byte(cursor), kind));
            index = cursor;
            continue;
        }
        if is_operator(ch) {
            while at(index).is_some_and(is_operator) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Operator));
            continue;
        }
        tokens.push((start..byte(index + 1), DiffTokenKind::Plain));
        index += 1;
    }
    tokens
}

/// `tab-size: 2`: each tab advances to the next even column. Returns the expanded text and a map
/// from the expanded text back to the source bytes, so token ranges can be carried over.
pub(crate) fn expand_tabs(content: &str) -> (String, Vec<usize>) {
    if !content.contains('\t') {
        return (content.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(content.len() + 8);
    // `offsets[source_byte]` = byte in `out`.
    let mut offsets = Vec::with_capacity(content.len() + 1);
    let mut column = 0usize;
    for ch in content.chars() {
        for _ in 0..ch.len_utf8() {
            offsets.push(out.len());
        }
        if ch == '\t' {
            let width = 2 - column % 2;
            out.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    offsets.push(out.len());
    (out, offsets)
}

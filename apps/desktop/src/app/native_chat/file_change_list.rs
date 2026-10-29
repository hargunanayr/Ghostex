//! A finished turn's changed files, at the end of the turn: a card shut by default to its
//! "N files changed" header with the total +/− and Open diff, which opens to a list of the files,
//! each with its folder, an A/M/D letter and its counts. A file row opens to its diff, one well per
//! write the turn made to it.
//!
//! The rows, their counts and order come from `packages/gx-chat-core/src/transcript/file_change_list.rs`
//! (the user's pick is the `CDXC:SessionChat 2026-10-27 DECISION` there); this file only lays it
//! out, with the message card's palette and diff well (`file_change_card.rs`).

use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    file_change_card::{Palette, counts, folder_label, icon, status_icon},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// The turn's changed files card, or nothing for a turn that changed none. The label and the
    /// list are projected together in the core (`transcript/presentation.rs`), so the label counts
    /// the files drawn.
    pub(super) fn completed_files_card(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let TranscriptItem::CompletedWork {
            id,
            files_card,
            files,
            files_label,
            simple_files_label,
            deferred,
            ..
        } = item
        else {
            return None;
        };
        // An older turn arrives with only the paths its unread work changed; the card reads that
        // work when it is opened.
        let unread = files.is_empty()
            && deferred
                .as_ref()
                .and_then(|work| work["filePaths"].as_array())
                .is_some_and(|paths| !paths.is_empty());
        if files.is_empty() && !unread {
            return None;
        }
        let palette = Palette::of(p);
        let s = p.scale;
        let key = format!("work-files:{id}");
        let expanded = self.is_expanded(&key, false);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let load = unread.then(|| json!({"type":"loadWork","id":id,"work":deferred}));
        let label = if p.simple {
            simple_files_label.clone()
        } else {
            files_label.clone()
        };
        let mut card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .text_color(p.card_muted)
            .child(
                self.files_card_header(&key, label, files_card, expanded, load, &palette, p, cx),
            );
        if expanded || motion.is_some() {
            let body = if unread {
                self.deferred_work_notice(item, p, cx)
            } else {
                let list = files_card["list"].as_array().cloned().unwrap_or_default();
                let rows: Vec<AnyElement> = list
                    .iter()
                    .map(|row| self.files_list_row(id, row, files, &palette, p, cx))
                    .collect();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .py(px(4.0 * s))
                        .border_t_1()
                        .border_color(palette.hairline)
                        .children(rows)
                        .into_any_element(),
                )
            };
            if let Some(body) = body {
                card = card.child(self.disclosure_body_motion(&key, motion, 0.0, body));
            }
        }
        Some(card.into_any_element())
    }

    /// Chevron, "N files changed", the turn's +/− and Open diff. The bar opens and shuts the card;
    /// a turn whose rows are still on the server reads them as it opens.
    #[allow(clippy::too_many_arguments)]
    fn files_card_header(
        &self,
        key: &str,
        label: String,
        summary: &Value,
        expanded: bool,
        load: Option<Value>,
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let toggle_key = key.to_string();
        let paths = summary["paths"].clone();
        let changed =
            summary["added"].as_u64().unwrap_or(0) + summary["removed"].as_u64().unwrap_or(0);
        div()
            .id(format!("{key}:header"))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .pl(px(12.0 * s))
            .pr(px(8.0 * s))
            .py(px(7.0 * s))
            .min_w_0()
            .chat_cursor_pointer()
            .hover(|style| style.bg(palette.hover))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx);
                if !expanded {
                    if let Some(action) = &load {
                        view.invoke(action.clone(), cx);
                    }
                }
            }))
            .child(icon(
                if expanded {
                    "titlebar/chevron-down.svg"
                } else {
                    "titlebar/chevron-right.svg"
                },
                13.0 * s,
                p.muted,
            ))
            .child(
                div()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .truncate()
                    .text_color(p.foreground)
                    .text_size(px(13.5 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .when(changed > 0, |this| {
                this.child(counts(
                    summary["added"].as_u64().unwrap_or(0),
                    summary["removed"].as_u64().unwrap_or(0),
                    palette,
                    s,
                ))
            })
            // The folder the rows' folders start from when the files are outside the project.
            .child(folder_label(text(summary, "listRoot"), p.muted, 12.0 * s))
            .child(div().flex_1())
            .child(
                div()
                    .id(format!("{key}:open-diff"))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.0 * s))
                    .h(px(28.0 * s))
                    .px(px(10.0 * s))
                    .rounded(px(8.0 * s))
                    .text_size(px(13.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .chat_filled_hover_text(p.neutral_button_fills(), true, Some(p.foreground))
                    .chat_cursor_pointer()
                    .child(icon("titlebar/git-compare.svg", 13.0 * s, p.muted))
                    .child("Open diff")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.invoke(json!({"type":"reviewFileChanges","paths":paths}), cx);
                    })),
            )
            .into_any_element()
    }

    /// A file's row, and under it while it is open the diff of every write the turn made to it.
    fn files_list_row(
        &mut self,
        turn: &str,
        row: &Value,
        files: &[Value],
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("list:{turn}:{}", text(row, "key"));
        let entries: Vec<usize> = row["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.as_u64().map(|entry| entry as usize))
            .filter(|entry| *entry < files.len())
            .collect();
        // Settings > Chat > Show file edit previews opens every file to its diff.
        let open = self.is_expanded(&key, p.file_previews);
        let status = row["status"].as_str().unwrap_or("edited");
        let hue = palette.status(status);
        let toggle_key = key.clone();
        let menu_path = text(row, "path");
        let header = div()
            .id(format!("{key}:row"))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .pl(px(14.0 * s))
            .pr(px(14.0 * s))
            .py(px(5.0 * s))
            .min_w_0()
            .chat_cursor_pointer()
            .hover(|style| style.bg(palette.hover))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), open, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.file_secondary_press(menu_path.clone(), event, window, cx);
                }),
            )
            .child(icon(status_icon(status), 14.0 * s, hue))
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_color(p.foreground)
                    .text_size(px(13.0 * s))
                    .child(text(row, "name")),
            )
            .child(folder_label(text(row, "folder"), p.muted, 12.0 * s))
            .when(row["failed"] == true, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.error())
                        .child("Failed"),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hue)
                    .child(text(row, "letter")),
            )
            .child(counts(
                row["added"].as_u64().unwrap_or(0),
                row["removed"].as_u64().unwrap_or(0),
                palette,
                s,
            ));
        let mut group = div().flex().flex_col().w_full().min_w_0().child(header);
        if !open {
            return group.into_any_element();
        }
        for entry in entries {
            let file = &files[entry];
            // The same key the message card's row asks with, so an open diff is read once.
            let diff_key = format!("file:work:{turn}:{entry}");
            let detail = self.row_detail(
                &diff_key,
                "file",
                file["messageId"].as_str().unwrap_or_default(),
                file["index"].as_u64().unwrap_or_default(),
            );
            if detail.is_null() {
                continue;
            }
            group = group.child(self.file_change_diff(&diff_key, file, &detail, palette, p, cx));
        }
        group.into_any_element()
    }
}

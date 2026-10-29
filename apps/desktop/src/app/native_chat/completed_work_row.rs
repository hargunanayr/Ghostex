//! A finished turn: its "Worked for 2m 5s" summary with the turn's counts beside it, the work log
//! behind it, the "N files changed" fold, the artifacts and answered questions it keeps in view,
//! and the final reply. What the summary says is decided in gx-chat-core `work_summary.rs`.
//!
//! At rest the row is one column with a 10px gap. While the
//! turn is folding, or its log is opening or closing, `disclosure_motion.rs` reports a frame and
//! the row is built from the same parts with each gap carried inside the part that owns it, so a
//! part whose height eases to nothing takes its gap with it and the first and last frames match
//! the live rows and the settled row exactly.

use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::disclosure_motion::{FoldFrame, FoldMotionKind, motion_clip};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

/// Space between the parts of a finished turn.
const PART_GAP: f32 = 10.0;
/// Space between two transcript list items, the `pb(16)` every row wrapper ends with, which the
/// live rows had between them before they folded.
const LIVE_ROW_GAP: f32 = 16.0;

/// A part whose height eases between nothing and its natural height; the fold applies its own
/// opacity to the rows, so the clip carries none.
fn eased_part(height: Rc<Cell<f32>>, factor: f32, content: AnyElement) -> AnyElement {
    motion_clip(
        height,
        FoldFrame {
            kind: FoldMotionKind::Fold,
            heading: 1.0,
            heading_offset: 0.0,
            log: factor,
            rows_opacity: 1.0,
        },
        0.0,
        0.0,
        content,
    )
}

impl NativeChatView {
    pub(super) fn completed_work_row(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Only a finished turn is drawn here (transcript.rs).
        let TranscriptItem::CompletedWork {
            id: turn,
            work,
            questions,
            sent_messages,
            workflows,
            artifacts,
            final_message,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let id = format!("work:{turn}");
        let expanded = self.is_expanded(&id, p.work_open);
        self.disclosure_motion
            .borrow_mut()
            .arrived(&id, !expanded, cx.reduce_motion());
        let motion = self.disclosure_frame(&id, expanded, cx);
        let heading = self.work_summary(item, &id, expanded, p, cx);
        // Every row of a finished turn hides its own writes; they belong to the turn's
        // "N files changed" card at its end (file_change_list.rs). Rows that are still
        // folding keep theirs until they are gone, as they were a moment ago.
        let folding = motion.is_some_and(|frame| frame.kind == FoldMotionKind::Fold);
        let log = if folding {
            let mut rows: Vec<AnyElement> = Vec::new();
            for message in work {
                rows.push(self.message_row(message, p, window, cx));
            }
            (!rows.is_empty()).then_some(rows)
        } else if expanded || motion.is_some() {
            // React hung the whole log off the rail the "Worked for" heading opened
            // (`SessionChatExpansion`), so the turn's work reads as one indented block.
            self.hide_file_changes = true;
            let mut log: Vec<AnyElement> = Vec::new();
            if let Some(notice) = self.deferred_work_notice(item, p, cx) {
                log.push(notice);
            }
            self.in_work_fold = true;
            for message in work {
                log.push(self.message_row(message, p, window, cx));
            }
            self.in_work_fold = false;
            (!log.is_empty()).then(|| {
                vec![disclosure_body(
                    p,
                    DisclosureRail::Work,
                    8.0,
                    id.clone(),
                    "Collapse completed work",
                    log,
                    cx,
                )]
            })
        } else {
            None
        };
        self.hide_file_changes = true;
        let files = self.completed_files_card(item, p, cx);
        let mut tail: Vec<AnyElement> = Vec::new();
        for message in artifacts {
            tail.push(self.message_row(message, p, window, cx));
        }
        // The turn's answered questions, lifted out of the fold so an exchange the reader took
        // part in is never buried by a collapsed "Worked for Xs" section.
        if let Some(cards) = self.question_exchange_cards(&id, questions, p, cx) {
            tail.push(cards);
        }
        if let Some(cards) = self.sent_agent_message_cards(sent_messages, p, cx) {
            tail.push(cards);
        }
        if let Some(cards) = self.workflow_cards(workflows, p, cx) {
            tail.push(cards);
        }
        if let Some(reply) = final_message.as_deref().filter(|reply| reply.is_object()) {
            tail.push(self.message_row(reply, p, window, cx));
        }
        self.hide_file_changes = false;

        let Some(frame) = motion else {
            let mut row = div()
                .flex()
                .flex_col()
                .w_full()
                .gap(px(PART_GAP * s))
                .child(heading);
            if let Some(log) = log {
                row = row.children(log);
            }
            // The changed files card closes the turn, under the reply (file_change_list.rs).
            return row.children(tail).children(files).into_any_element();
        };
        self.moving_completed_work_row(&id, frame, heading, log, files, tail, s)
    }

    /// The bar a finished turn folds behind: a clock tile with a green check badge, "Worked for
    /// 2m 5s" and the turn's counts as pills, so the closed fold still says what happened; the
    /// whole bar opens and closes it, with no chevron. Pressing it opens the log, reading a turn
    /// whose rows are still on the server first.
    ///
    /// Its height, fill and type are every one-line card's (`SINGLE_LINE_CARD`, cards.rs).
    ///
    /// CDXC:SessionChat 2026-10-11 DECISION: User: move the count pills to the right side of the "Worked …" row and give them fully rounded (pill) corners. They wrap among themselves, right-aligned, when the row is narrow; the phone's `TranscriptItemRow.tsx` lays them out the same way.
    fn work_summary(
        &self,
        item: &TranscriptItem,
        id: &str,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let TranscriptItem::CompletedWork {
            id: turn,
            label,
            stats,
            expandable,
            deferred,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let expandable = *expandable;
        let label = label.clone();
        let stats: Vec<(String, String)> = stats
            .iter()
            .map(|stat| (text(stat, "kind"), text(stat, "label")))
            .collect();
        let spoken = std::iter::once(label.clone())
            .chain(stats.iter().map(|(_, label)| label.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        let card = &*SINGLE_LINE_CARD;
        let chip = |kind: &str, label: String| {
            let failed = matches!(kind, "failed" | "recovered");
            let tone = if failed { p.error() } else { p.card_muted };
            let icon = match kind {
                "files" => "titlebar/pencil.svg",
                "failed" | "recovered" => "titlebar/alert-triangle.svg",
                _ => "titlebar/tool.svg",
            };
            card.pill(Some(icon), Some(label), tone, p)
        };
        let mut bar = card
            .shell(div(), p)
            .id(SharedString::from(format!("{id}:summary")))
            .role(gpui::Role::Button)
            .aria_label(spoken)
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .w_full()
            .h(px(card.height * s))
            .flex_shrink_0()
            .gap(px(10.0 * s))
            .px(px(card.padding_x * s))
            .child(
                card.tile("titlebar/chat-card-clock.svg", p)
                    .child(card.check_badge(p)),
            )
            .child(
                card.title(div().flex_shrink_0(), p)
                    .text_color(p.foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .justify_end()
                    .items_center()
                    .gap(px(6.0 * s))
                    .children(stats.into_iter().map(|(kind, label)| chip(&kind, label))),
            );
        if expandable {
            let id = id.to_string();
            let action = deferred
                .as_ref()
                .filter(|work| work.is_object())
                .map(|work| json!({"type":"loadWork","id":turn,"work":work}));
            bar = bar
                .chat_cursor_pointer()
                .hover(|style| style.bg(card_hover_fill(p)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.anchor_disclosure_toggle(&id, !expanded);
                    if expanded {
                        this.expanded.remove(&id);
                        this.collapsed.insert(id.clone());
                    } else {
                        this.collapsed.remove(&id);
                        this.expanded.insert(id.clone());
                        if let Some(action) = &action {
                            this.invoke(action.clone(), cx);
                        }
                    }
                    cx.notify();
                }));
        }
        bar.into_any_element()
    }

    /// The row while it folds, opens or closes. Each part carries the gap above it, so a part
    /// eased to nothing leaves no gap behind.
    #[allow(clippy::too_many_arguments)]
    fn moving_completed_work_row(
        &mut self,
        id: &str,
        frame: FoldFrame,
        heading: AnyElement,
        log: Option<Vec<AnyElement>>,
        files: Option<AnyElement>,
        tail: Vec<AnyElement>,
        s: f32,
    ) -> AnyElement {
        let spaced = |part: AnyElement| div().pt(px(PART_GAP * s)).child(part).into_any_element();
        let mut row = div().flex().flex_col().w_full();
        let top = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .top(px(-frame.heading_offset * s))
                    .opacity(frame.heading)
                    .child(heading),
            )
            .into_any_element();
        if frame.kind == FoldMotionKind::Fold {
            // The heading did not exist while the turn was live, so it grows in; the rows it
            // replaces keep the spacing they had as separate transcript rows until they are gone.
            row = row.child(eased_part(
                self.disclosure_motion.borrow_mut().height(id, "heading"),
                frame.heading,
                top,
            ));
            if let Some(rows) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(LIVE_ROW_GAP * s))
                        .pb(px((LIVE_ROW_GAP - PART_GAP) * s))
                        .opacity(frame.rows_opacity)
                        .children(rows)
                        .into_any_element(),
                ));
            }
            row = row.children(tail.into_iter().map(spaced));
            if let Some(files) = files {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "files"),
                    frame.heading,
                    spaced(files),
                ));
            }
        } else {
            row = row.child(top);
            if let Some(log) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .pt(px(PART_GAP * s))
                        .opacity(frame.rows_opacity)
                        .children(log)
                        .into_any_element(),
                ));
            }
            row = row
                .children(tail.into_iter().map(spaced))
                .children(files.map(spaced));
        }
        row.into_any_element()
    }
}

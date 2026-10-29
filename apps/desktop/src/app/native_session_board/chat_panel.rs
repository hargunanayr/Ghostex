//! The chat beside the Board's lanes: the card's column, title, project and agent, its Complete or
//! Reopen, and the thread's own chat, the same one its Agents pane shows.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): clicking a card opens its chat at the side of the Board, like PandaCode's, instead of leaving the Board for the thread. This supersedes the same day's rule that a card's click left the Board. A task still waiting for Start opens its editor instead, because focusing a draft would start its agent.

use ghostex_gx_core::session_board::{BoardCard, BoardColumn};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{h_flex, v_flex};

use super::card::{agent_icon, status_color};
use super::view::lane_tone;
use crate::GhostexGpuiApp;
use crate::app::helpers::{titlebar_svg_icon, titlebar_tooltip};
use crate::app::model::*;
use crate::app::native_chat::appearance::ChatAppearance;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::{KanbanButtonKind, kanban_button};
use crate::*;

/// The share of the window the chat takes, and its bounds.
const CHAT_PANEL_SHARE: f32 = 0.42;
const CHAT_PANEL_MIN_WIDTH: f32 = 420.0;
const CHAT_PANEL_MAX_WIDTH: f32 = 760.0;

impl GhostexGpuiApp {
    /// The pane and shell session of the Board's chat thread, once its focus has mapped it into the
    /// Agents workspace.
    pub(crate) fn session_board_chat_target(&self) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        let card = self.native_session_board.chat.as_ref()?;
        let key = &card.key;
        let session_id = match key.machine.remote_id() {
            None => self
                .local_workspace_session_mappings
                .get(&GpuiLocalWorkspaceSessionKey {
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                })
                .copied(),
            Some(machine_id) => self
                .remote_attach_sessions
                .get(&GpuiRemoteAttachSessionKey {
                    remote_machine_id: machine_id.to_string(),
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                })
                .copied(),
        }?;
        let pane_id = self.agents_workspace.pane_id_for_session(session_id)?;
        Some((pane_id, session_id))
    }

    pub(crate) fn render_session_board_chat_panel(
        &mut self,
        card: &BoardCard,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let width = (window.viewport_size().width.as_f32() * CHAT_PANEL_SHARE)
            .clamp(CHAT_PANEL_MIN_WIDTH, CHAT_PANEL_MAX_WIDTH);
        let light = ChatAppearance::current(&serde_json::Value::Null).light;
        let body = match self.session_board_chat_target() {
            Some((pane_id, session_id)) => {
                // The focus behind the Board mapped the thread after the chat panel opened; the
                // coalesced pass that makes chat views makes this one now.
                if !self.native_chat_views.contains_key(&session_id) {
                    self.reconcile_agents_pane_surfaces(cx);
                }
                self.render_agents_session_chat_body(pane_id, session_id, cx)
            }
            None => self.render_session_chat_skeleton(),
        };
        let column_label = match card.status.label() {
            Some(status) => format!("{} · {status}", card.column.label()),
            None => card.column.label().to_string(),
        };
        let status_tone = status_color(card.status, p).unwrap_or(p.muted);
        let action = if card.can_complete() {
            Some(("titlebar/check.svg", "Complete", true))
        } else if card.can_reopen() {
            Some(("titlebar/arrow-back-up.svg", "Reopen", false))
        } else {
            None
        };
        let agent_name = card.row.menu_facts.agent_name.clone();
        v_flex()
            .id("session-board-chat-panel")
            .flex_none()
            .w(px(width))
            .h_full()
            .min_h_0()
            .bg(p.panel)
            .border_l_1()
            .border_color(p.border_strong)
            .font_family(p.font.clone())
            .text_color(p.foreground)
            .child(
                v_flex()
                    .flex_none()
                    .gap(px(8.0))
                    .px(px(20.0))
                    .pt(px(16.0))
                    .pb(px(14.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(div().size(px(7.0)).rounded_full().bg(lane_tone(card.column)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .text_color(if card.column == BoardColumn::Review {
                                        status_tone
                                    } else {
                                        p.muted
                                    })
                                    .child(column_label),
                            )
                            .child(
                                kanban_button(
                                    "session-board-chat-open-in-chats",
                                    Some("titlebar/message.svg"),
                                    None,
                                    KanbanButtonKind::Ghost,
                                    false,
                                    p,
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.session_board_open_in_chats(cx);
                                }))
                                .managed_tooltip_with_placement(
                                    ManagedTooltipPlacement::Auto,
                                    |window, cx| titlebar_tooltip("Open in chats", window, cx),
                                ),
                            )
                            .child(
                                kanban_button(
                                    "session-board-chat-close",
                                    Some("titlebar/x.svg"),
                                    None,
                                    KanbanButtonKind::Ghost,
                                    false,
                                    p,
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.session_board_close_chat(cx);
                                })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .line_height(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_clamp(2)
                            .child(card.title().to_string()),
                    )
                    .child(
                        h_flex()
                            .gap(px(7.0))
                            .min_w_0()
                            .items_center()
                            .text_size(px(12.5))
                            .text_color(p.muted)
                            .child(titlebar_svg_icon("titlebar/folder.svg", 13.0, p.muted))
                            .child(div().truncate().min_w_0().child(card.project_label()))
                            .when_some(agent_name, |this, name| {
                                this.child(div().text_color(p.faint).child("·"))
                                    .child(agent_icon(card, light))
                                    .child(div().truncate().min_w_0().child(name))
                            }),
                    )
                    .when_some(action, |this, (icon, label, complete)| {
                        let card = card.clone();
                        this.child(
                            h_flex().pt(px(4.0)).child(
                                kanban_button(
                                    "session-board-chat-action",
                                    Some(icon),
                                    Some(label.into()),
                                    if complete {
                                        KanbanButtonKind::Primary
                                    } else {
                                        KanbanButtonKind::Secondary
                                    },
                                    false,
                                    p,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.session_board_set_completed(&card, complete, cx);
                                })),
                            ),
                        )
                    }),
            )
            // A flex column, like the Agents pane the chat body comes from: its frame takes its
            // height with `flex_1`, and in a plain block it collapsed to nothing.
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .child(body),
            )
            .into_any_element()
    }
}

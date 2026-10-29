//! One thread's card: its title, a task's prompt, the project and agent, the status that matters
//! for triage, and the one action its column offers (Start, Complete or Reopen).

use ghostex_gx_core::colored_agent_logo;
use ghostex_gx_core::session_board::{BoardCard, BoardCardStatus, BoardColumn};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, ElementId, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, img, px, rgb,
};

use super::view::NativeSessionBoardView;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_sidebar::images::agent_image;

const CARD_RADIUS: f32 = 9.0;

/// The status colours the sidebar draws its dots in, so a pink or blue card reads the same there.
///
/// CDXC:SessionBoard 2026-10-28 SEE-ALSO: apps/desktop/src/app/native_sidebar/two_column/card_meta.rs (the same pink, blue and working colours on the sidebar's cards).
pub(super) fn status_color(status: BoardCardStatus, p: &KanbanPalette) -> Option<Hsla> {
    match status {
        BoardCardStatus::NeedsAnswer => Some(rgb(0xf472b6).into()),
        BoardCardStatus::Unseen => Some(rgb(0x95d7f6).into()),
        BoardCardStatus::Working | BoardCardStatus::Starting => Some(rgb(0xd9a23a).into()),
        BoardCardStatus::Error => Some(p.danger),
        BoardCardStatus::Sleeping | BoardCardStatus::None => None,
    }
}

/// What a dragged Review or Completed card carries; those two lanes take it.
#[derive(Clone)]
pub(crate) struct BoardCardDrag {
    pub(crate) sidebar_session_id: String,
    title: SharedString,
    font: SharedString,
    background: Hsla,
    border: Hsla,
    foreground: Hsla,
}

impl Render for BoardCardDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.0))
            .px(px(13.0))
            .py(px(11.0))
            .rounded(px(CARD_RADIUS))
            .bg(self.background)
            .border_1()
            .border_color(self.border)
            .shadow_lg()
            .opacity(0.94)
            .font_family(self.font.clone())
            .text_size(px(13.0))
            .text_color(self.foreground)
            .truncate()
            .child(self.title.clone())
    }
}

pub(super) fn agent_icon(card: &BoardCard, light: bool) -> AnyElement {
    let image = card
        .row
        .agent_icon
        .as_deref()
        .and_then(|icon| colored_agent_logo(icon).map(|url| (icon, url)))
        .and_then(|(icon, url)| agent_image(url, Some(icon), light));
    match image {
        Some(image) => img(image).size(px(13.0)).flex_none().into_any_element(),
        None => div().size(px(13.0)).flex_none().into_any_element(),
    }
}

/// A small bordered button on a card.
fn card_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: &'static str,
    p: &KanbanPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.0))
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.control)
        .text_size(px(12.0))
        .text_color(p.foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(titlebar_svg_icon(icon, 12.0, p.foreground))
        .child(label)
}

/// The card's git line: the branch, then the worktree (by name unless it repeats the branch).
fn git_line(card: &BoardCard, p: &KanbanPalette) -> Option<AnyElement> {
    if card.branch.is_none() && card.worktree.is_none() {
        return None;
    }
    let worktree = card.worktree.as_ref().map(|name| {
        if card.branch.as_ref() == Some(name) {
            "Worktree".to_string()
        } else {
            format!("Worktree {name}")
        }
    });
    let has_branch = card.branch.is_some();
    Some(
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .min_w_0()
            .text_size(px(11.5))
            .text_color(p.faint)
            .when_some(card.branch.clone(), |this, branch| {
                this.child(titlebar_svg_icon("titlebar/git-branch.svg", 11.0, p.faint))
                    .child(div().truncate().min_w_0().child(branch))
            })
            .when_some(worktree, |this, worktree| {
                this.when(has_branch, |this| {
                    this.child(div().flex_none().opacity(0.6).child("·"))
                })
                .child(titlebar_svg_icon("titlebar/git-fork.svg", 11.0, p.faint))
                .child(div().truncate().min_w_0().child(worktree))
            })
            .into_any_element(),
    )
}

impl NativeSessionBoardView {
    pub(super) fn render_card(
        &self,
        card: &BoardCard,
        p: &KanbanPalette,
        light: bool,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = card.sidebar_session_id.clone();
        let group: SharedString = format!("session-board-card-{id}").into();
        let busy = self.is_busy(&id);
        let selected = self.chat_card.as_deref() == Some(id.as_str());
        let status_tone = status_color(card.status, p);
        let draggable = matches!(card.column, BoardColumn::Review | BoardColumn::Completed);
        let hover = p.card_hover;
        let time = card.row.last_interaction_label(now_ms);
        let action = if card.task_waiting {
            Some(("start", "titlebar/player-play.svg", "Start", true))
        } else if card.can_complete() {
            Some(("complete", "titlebar/check.svg", "Complete", false))
        } else if card.can_reopen() {
            Some(("reopen", "titlebar/arrow-back-up.svg", "Reopen", false))
        } else {
            None
        };
        let drag = BoardCardDrag {
            sidebar_session_id: id.clone(),
            title: card.title().to_string().into(),
            font: p.font.clone().into(),
            background: p.card_hover,
            border: p.border_strong,
            foreground: p.foreground,
        };
        let click_id = id.clone();
        div()
            .id(ElementId::Name(format!("session-board-card-{id}").into()))
            .group(group.clone())
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .px(px(13.0))
            .py(px(11.0))
            .rounded(px(CARD_RADIUS))
            .bg(if selected { p.card_hover } else { p.card })
            .border_1()
            .border_color(if selected {
                p.border_strong
            } else {
                gpui::transparent_black()
            })
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(busy, |this| this.opacity(0.55))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.card_clicked(&click_id, window, cx);
            }))
            .when(draggable, |this| {
                this.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .min_w_0()
                    .when_some(
                        status_tone.filter(|_| {
                            matches!(
                                card.status,
                                BoardCardStatus::NeedsAnswer | BoardCardStatus::Unseen
                            )
                        }),
                        |this, tone| {
                            this.child(div().size(px(7.0)).flex_none().rounded_full().bg(tone))
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child(card.title().to_string()),
                    ),
            )
            .when_some(card.task_preview.clone(), |this, preview| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.0))
                        .text_color(p.muted)
                        .line_clamp(2)
                        .child(preview),
                )
            })
            .when_some(git_line(card, p), |this, line| this.child(line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .min_w_0()
                    .min_h(px(24.0))
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .child(agent_icon(card, light))
                    .child(div().truncate().min_w_0().child(card.project_label()))
                    .when_some(card.thread_of.clone(), |this, parent| {
                        this.child(
                            div()
                                .truncate()
                                .min_w_0()
                                .text_color(p.faint)
                                .child(format!("· thread of {parent}")),
                        )
                    })
                    .when_some(card.status.label(), |this, label| {
                        this.child(
                            div()
                                .flex_none()
                                .text_color(status_tone.unwrap_or(p.muted))
                                .child(label),
                        )
                    })
                    .child(div().flex_1())
                    .when_some(action, |this, (kind, icon, label, always)| {
                        let action_id = id.clone();
                        let button = card_button(
                            ElementId::Name(format!("session-board-{kind}-{id}").into()),
                            icon,
                            label,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.card_action(&action_id, window, cx);
                        }));
                        if always {
                            this.child(button)
                        } else {
                            this.child(
                                div()
                                    .opacity(0.0)
                                    .group_hover(group.clone(), |this| this.opacity(1.0))
                                    .child(button),
                            )
                        }
                    })
                    .when_some(time.filter(|_| !card.task_waiting), |this, time| {
                        this.child(
                            div()
                                .flex_none()
                                .text_color(p.faint)
                                .child(time),
                        )
                    }),
            )
            .into_any_element()
    }
}

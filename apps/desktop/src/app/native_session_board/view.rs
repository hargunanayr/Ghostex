//! The Board page: the project filter and Add task, then the four lanes side by side, and the task
//! panel beside them while it is open. A card's chat beside them is the app's (`chat_panel.rs`).
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): a Board like PandaCode's (the Beads Kanban stays as it is), laid out "seperti layout awal saja, dimana ada 4 kolom kesamping". Desktop only for now; the phone gets its Board later (this sets aside, for the Board, the 2026-10-09 rule that desktop changes reach the phone). Review and Completed take dropped cards (Complete and Reopen); Pending and In progress follow the thread's own state. Where it opens is in host.rs.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use ghostex_gx_core::session_board::{
    BoardCard, BoardColumn, BoardDrop, BoardFilter, SessionBoardView, board_drop,
    complete_all_targets,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, WeakEntity, Window, div, px, rgb,
};

use super::card::BoardCardDrag;
use super::panel::TaskPanel;
use super::picker::{PickerTarget, ProjectPicker};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_chat::appearance::ChatAppearance;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::{
    KanbanButtonKind, LANE_RADIUS, anchor_probe, kanban_button,
};

/// How often the cards' relative times are redrawn.
const CLOCK_INTERVAL: Duration = Duration::from_secs(30);
/// A lane draws at most this many cards and says how many more there are.
const LANE_CARD_LIMIT: usize = 200;
/// How long a card stays faded waiting for its action when the store never moves it.
const BUSY_FOR: Duration = Duration::from_secs(6);

pub(super) fn lane_tone(column: BoardColumn) -> Hsla {
    match column {
        BoardColumn::Pending => rgb(0x8f9aa7).into(),
        BoardColumn::InProgress => rgb(0xe7b85b).into(),
        BoardColumn::Review => rgb(0xb18cff).into(),
        BoardColumn::Completed => rgb(0x6fd19c).into(),
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

pub(crate) struct NativeSessionBoardView {
    pub(super) app: WeakEntity<GhostexGpuiApp>,
    pub(super) board: SessionBoardView,
    filter: BoardFilter,
    /// The Board has had its first build; until then the lanes draw empty.
    loaded: bool,
    /// The page holds focus while a menu is open, so the pick is dispatched to it.
    pub(super) focus: FocusHandle,
    filter_trigger: Rc<Cell<Bounds<Pixels>>>,
    space_trigger: Rc<Cell<Bounds<Pixels>>>,
    pub(super) panel: Option<TaskPanel>,
    /// The searchable project picker, while it is open.
    pub(super) picker: Option<ProjectPicker>,
    /// The card whose chat is open beside the lanes (the app's `NativeSessionBoardHost::chat`).
    pub(super) chat_card: Option<String>,
    /// The lane whose bulk action (Complete all, Close all) is asking for a second click.
    confirm: Option<BoardColumn>,
    /// Cards whose action is on its way to gxserver, with when it was sent; they draw faded until
    /// the store moves them, or for [`BUSY_FOR`] at most when the action failed.
    busy: BTreeMap<String, u64>,
    glass: bool,
    _clock: Task<()>,
}

impl NativeSessionBoardView {
    pub(crate) fn new(app: WeakEntity<GhostexGpuiApp>, cx: &mut Context<Self>) -> Self {
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CLOCK_INTERVAL).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            app,
            board: SessionBoardView::default(),
            filter: BoardFilter::All,
            loaded: false,
            focus: cx.focus_handle(),
            filter_trigger: Rc::new(Cell::new(Bounds::default())),
            space_trigger: Rc::new(Cell::new(Bounds::default())),
            panel: None,
            picker: None,
            chat_card: None,
            confirm: None,
            busy: BTreeMap::new(),
            glass: false,
            _clock: clock,
        }
    }

    pub(crate) fn filter(&self) -> &BoardFilter {
        &self.filter
    }

    /// True when the page must be redrawn because the window glass changed.
    pub(crate) fn sync(&mut self, glass: bool) -> bool {
        std::mem::replace(&mut self.glass, glass) != glass
    }

    pub(crate) fn set_board(&mut self, board: SessionBoardView, cx: &mut Context<Self>) {
        if self.loaded && self.board == board {
            return;
        }
        self.filter = board.filter.clone();
        // A card the store moved or closed is done waiting.
        self.busy.retain(|id, _| {
            board
                .card(id)
                .is_some_and(|card| self.board.card(id).is_some_and(|old| old == card))
        });
        self.board = board;
        self.loaded = true;
        if let Some(panel) = &self.panel
            && let Some(id) = panel.editing()
            && self.board.card(id).is_none_or(|card| !card.task_waiting)
        {
            self.panel = None;
        }
        cx.notify();
    }

    pub(super) fn is_busy(&self, sidebar_session_id: &str) -> bool {
        self.busy
            .get(sidebar_session_id)
            .is_some_and(|sent| now_ms().saturating_sub(*sent) < BUSY_FOR.as_millis() as u64)
    }

    /// Fades `ids` until the store moves them, and redraws once [`BUSY_FOR`] has passed in case it
    /// never does.
    fn mark_busy<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>, cx: &mut Context<Self>) {
        let now = now_ms();
        for id in ids {
            self.busy.insert(id.to_string(), now);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BUSY_FOR).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    pub(super) fn card_by_id(&self, sidebar_session_id: &str) -> Option<BoardCard> {
        self.board.card(sidebar_session_id).cloned()
    }

    /// Runs `f` on the app after this update, so an app call that redraws the Board does not find
    /// the page mid-update.
    pub(super) fn app_later(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut GhostexGpuiApp, &mut Window, &mut Context<GhostexGpuiApp>) + 'static,
    ) {
        let app = self.app.clone();
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| f(app, window, cx));
        });
    }

    pub(super) fn set_filter(
        &mut self,
        filter: BoardFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        self.confirm = None;
        self.app_later(window, cx, |app, _, cx| app.rebuild_native_session_board(cx));
    }

    fn show_filter_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.filter_trigger.get();
        self.toggle_project_picker(PickerTarget::Filter, trigger, window, cx);
    }

    fn show_space_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.space_trigger.get();
        self.toggle_project_picker(PickerTarget::Space, trigger, window, cx);
    }

    /// The app opened or closed a card's chat.
    pub(crate) fn set_chat_card(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.chat_card != id {
            self.chat_card = id;
            cx.notify();
        }
    }

    /// The chat beside the lanes gives way to the task panel.
    pub(super) fn close_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat_card.take().is_some() {
            self.app_later(window, cx, |app, _, cx| app.session_board_close_chat(cx));
        }
    }

    /// One of the page's fields took focus: the page is GPUI's own, so nothing else keeps the keys.
    pub(super) fn input_focused(&mut self, cx: &mut Context<Self>) {
        let _ = self
            .app
            .update(cx, |app, _| app.session_board_input_focused());
    }

    /// A click on a card: a task waiting for Start opens its editor, any other card its chat beside
    /// the lanes.
    pub(super) fn card_clicked(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.card_by_id(id) else {
            return;
        };
        if card.task_waiting {
            self.open_edit_panel(&card, window, cx);
            return;
        }
        self.panel = None;
        self.picker = None;
        self.chat_card = Some(card.sidebar_session_id.clone());
        cx.notify();
        self.app_later(window, cx, move |app, _, cx| {
            app.session_board_open_chat(card, cx);
        });
    }

    /// The card's own button: Start, Complete or Reopen.
    pub(super) fn card_action(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.card_by_id(id) else {
            return;
        };
        self.mark_busy([card.sidebar_session_id.as_str()], cx);
        self.app_later(window, cx, move |app, _, cx| {
            if card.task_waiting {
                app.session_board_start_task(&card, cx);
            } else {
                app.session_board_set_completed(&card, card.can_complete(), cx);
            }
        });
    }

    fn card_dropped(
        &mut self,
        drag: &BoardCardDrag,
        to: BoardColumn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.card_by_id(&drag.sidebar_session_id) else {
            return;
        };
        let Some(drop) = board_drop(&card, to) else {
            return;
        };
        self.mark_busy([card.sidebar_session_id.as_str()], cx);
        self.app_later(window, cx, move |app, _, cx| {
            app.session_board_set_completed(&card, drop == BoardDrop::Complete, cx);
        });
    }

    /// Complete all and Close all ask once more in the lane head; the second click runs them.
    fn bulk_clicked(&mut self, column: BoardColumn, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirm != Some(column) {
            self.confirm = Some(column);
            cx.notify();
            return;
        }
        self.confirm = None;
        let board = self.board.clone();
        let ids: Vec<&str> = board
            .lane(column)
            .into_iter()
            .flat_map(|lane| &lane.cards)
            .filter(|card| column == BoardColumn::Completed || !card.waits_on_answer())
            .map(|card| card.sidebar_session_id.as_str())
            .collect();
        self.mark_busy(ids, cx);
        let board = self.board.clone();
        self.app_later(window, cx, move |app, _, cx| match column {
            BoardColumn::Review => app.session_board_complete_all(&board, cx),
            BoardColumn::Completed => app.session_board_close_completed(&board, cx),
            BoardColumn::Pending | BoardColumn::InProgress => {}
        });
    }

    fn render_header(&self, p: &KanbanPalette, cx: &mut Context<Self>) -> AnyElement {
        let caption = match &self.board.space_name {
            Some(space) => format!("Board · {space}"),
            None => "Board".to_string(),
        };
        let note = format!("{} threads", self.board.total);
        div()
            .flex()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(self.render_space_caption(caption, p, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(self.render_filter_trigger(p, cx))
                            .child(div().text_size(px(13.0)).text_color(p.faint).child(note)),
                    ),
            )
            .child(div().flex_1())
            .child(
                kanban_button(
                    "session-board-add-task",
                    Some("titlebar/plus.svg"),
                    Some("Add task".into()),
                    KanbanButtonKind::Primary,
                    false,
                    p,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_new_panel(window, cx))),
            )
            .into_any_element()
    }

    /// "Board · <Space>", which opens the Space picker while the machine has Spaces.
    fn render_space_caption(
        &self,
        caption: String,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.5))
            .text_color(p.muted)
            .child(titlebar_svg_icon("titlebar/layout-kanban.svg", 13.0, p.muted))
            .child(caption);
        if self.board.spaces.is_empty() {
            return row.into_any_element();
        }
        let open = self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target() == PickerTarget::Space);
        let hover = p.foreground;
        div()
            .child(
                row.id("session-board-space-switcher")
                    .relative()
                    .cursor_pointer()
                    .hover(move |style| style.text_color(hover))
                    .child(anchor_probe(self.space_trigger.clone()))
                    .child(titlebar_svg_icon(
                        if open {
                            "titlebar/chevron-up.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        },
                        12.0,
                        p.muted,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| this.show_space_picker(window, cx))),
            )
            .into_any_element()
    }

    /// The project filter's title: All projects or the project, opening the searchable picker.
    fn render_filter_trigger(&self, p: &KanbanPalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .child(
                div()
                    .id("session-board-project-filter")
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(anchor_probe(self.filter_trigger.clone()))
                    .child(self.board.filter_title())
                    .child(titlebar_svg_icon(
                        if self
                            .picker
                            .as_ref()
                            .is_some_and(|picker| picker.target() == PickerTarget::Filter)
                        {
                            "titlebar/chevron-up.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        },
                        14.0,
                        p.muted,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| this.show_filter_picker(window, cx))),
            )
            .into_any_element()
    }

    fn render_lane_action(
        &self,
        column: BoardColumn,
        count: usize,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let label: SharedString = match column {
            BoardColumn::Pending => {
                return Some(
                    kanban_button(
                        "session-board-pending-add",
                        Some("titlebar/plus.svg"),
                        None,
                        KanbanButtonKind::Ghost,
                        false,
                        p,
                    )
                    .h(px(24.0))
                    .w(px(24.0))
                    .on_click(cx.listener(|this, _, window, cx| this.open_new_panel(window, cx)))
                    .into_any_element(),
                );
            }
            BoardColumn::InProgress => return None,
            BoardColumn::Review => {
                let (targets, kept) = complete_all_targets(&self.board);
                if targets.is_empty() {
                    return None;
                }
                if self.confirm == Some(column) {
                    if kept > 0 {
                        format!("Complete {} (keep {kept})", targets.len()).into()
                    } else {
                        format!("Complete {}?", targets.len()).into()
                    }
                } else {
                    "Complete all".into()
                }
            }
            BoardColumn::Completed => {
                if count == 0 {
                    return None;
                }
                if self.confirm == Some(column) {
                    format!("Close {count}?").into()
                } else {
                    "Close all".into()
                }
            }
        };
        let confirming = self.confirm == Some(column);
        let hover = p.control_hover;
        Some(
            div()
                .id(ElementId::Name(format!("session-board-bulk-{}", column.as_str()).into()))
                .flex()
                .items_center()
                .h(px(24.0))
                .px(px(9.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if confirming { p.border_strong } else { p.border })
                .when(confirming, |this| this.bg(p.control))
                .text_size(px(12.0))
                .text_color(if confirming { p.foreground } else { p.muted })
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.bulk_clicked(column, window, cx)
                }))
                .child(label)
                .into_any_element(),
        )
    }

    fn render_lane(
        &self,
        column: BoardColumn,
        p: &KanbanPalette,
        light: bool,
        now: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cards = self
            .board
            .lane(column)
            .map(|lane| lane.cards.as_slice())
            .unwrap_or_default();
        let droppable = matches!(column, BoardColumn::Review | BoardColumn::Completed);
        let drop_bg = p.lane_drop;
        let hidden = cards.len().saturating_sub(LANE_CARD_LIMIT);
        let action = self.render_lane_action(column, cards.len(), p, cx);
        div()
            .id(ElementId::Name(format!("session-board-lane-{}", column.as_str()).into()))
            .flex_1()
            .min_w_0()
            .max_h_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(LANE_RADIUS))
            .bg(p.lane)
            .border_1()
            .border_color(p.border)
            .when(droppable, |this| {
                this.drag_over::<BoardCardDrag>(move |style, _, _, _| style.bg(drop_bg))
                    .on_drop(cx.listener(move |this, drag: &BoardCardDrag, window, cx| {
                        this.card_dropped(drag, column, window, cx);
                    }))
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(26.0))
                    .px(px(4.0))
                    .child(div().size(px(7.0)).rounded_full().bg(lane_tone(column)))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child(column.label()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(p.faint)
                            .child(cards.len().to_string()),
                    )
                    .child(div().flex_1())
                    .children(action),
            )
            .child(
                div()
                    .id(ElementId::Name(
                        format!("session-board-lane-cards-{}", column.as_str()).into(),
                    ))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(cards.is_empty(), |this| {
                        this.child(
                            div()
                                .h(px(54.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.0))
                                .border_1()
                                .border_dashed()
                                .border_color(p.border_strong)
                                .text_size(px(12.0))
                                .text_color(p.faint)
                                .child(column.empty_label()),
                        )
                    })
                    .children(
                        cards
                            .iter()
                            .take(LANE_CARD_LIMIT)
                            .map(|card| self.render_card(card, p, light, now, cx)),
                    )
                    .when(hidden > 0, |this| {
                        this.child(
                            div()
                                .py(px(6.0))
                                .text_size(px(12.0))
                                .text_color(p.faint)
                                .child(format!("{hidden} more")),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl Render for NativeSessionBoardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = KanbanPalette::current(window);
        let light = ChatAppearance::current(&serde_json::Value::Null).light;
        let now = now_ms();
        let lanes: Vec<AnyElement> = BoardColumn::ORDER
            .iter()
            .map(|column| self.render_lane(*column, &p, light, now, cx))
            .collect();
        let panel = self.panel.is_some().then(|| self.render_panel(&p, window, cx));
        let picker = self.render_project_picker(&p, cx);
        div()
            .id("session-board")
            .key_context("SessionBoard")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::pick_task_agent))
            .on_action(cx.listener(Self::pick_task_model))
            .on_action(cx.listener(Self::pick_task_effort))
            .size_full()
            .flex()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_color(p.foreground)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .px(px(28.0))
                    .pt(px(22.0))
                    .pb(px(20.0))
                    .child(self.render_header(&p, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .items_start()
                            .gap(px(14.0))
                            .children(lanes),
                    ),
            )
            .children(panel)
            .children(picker)
    }
}

//! The app side of the Board: the window-wide mode that covers the workarea with it, the header
//! row it draws in place of the workarea header, and the chat it opens beside its lanes.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): the Board is not a side panel view ("jangan dijadikan sebagai side panel") but a button at the top left next to Toggle sidebar, so it is not tied to the project it was opened in: it is the alternative to the chat view. It covers the whole workarea (the Agents column, the view panel and the bottom panel) in every project until it is left by the same button or the Agents Panel (chat) button beside it, and the views each project had open are untouched underneath. This supersedes the same day's rule that made it a view panel tab beside the Kanban.
//! User (Ryan): "saat mode board, gw pengen full screen (Sidebar di kiri juga ikut di hide)". Opening the Board still folds the sidebar away, and leaving it puts back the sidebar it found.

use ghostex_gx_core::session_board::BoardCard;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, StyleRefinement, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

use super::view::NativeSessionBoardView;
use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::helpers::window_glass_active_in;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::render::window_drag_region::window_drag_region;

/// What the app keeps for the Board between draws.
#[derive(Default)]
pub(crate) struct NativeSessionBoardHost {
    pub(crate) view: Option<Entity<NativeSessionBoardView>>,
    /// A rebuild is waiting out the coalescing delay (`gx_store/session_board.rs`).
    pub(crate) rebuild_pending: bool,
    /// The sessions moved while the Board was not showing; opening it rebuilds it.
    pub(crate) stale: bool,
    /// The Board covers the workarea.
    pub(crate) showing: bool,
    /// The thread whose chat is open beside the lanes, as its card last read.
    pub(crate) chat: Option<BoardCard>,
    /// The sidebar's state when the Board opened, put back when it is left.
    restore_sidebar_collapsed: Option<bool>,
}

impl GhostexGpuiApp {
    pub(crate) fn session_board_showing(&self) -> bool {
        self.native_session_board.showing
    }

    /// The Board button is offered unless Settings hides it (`sessionBoardHidden`).
    pub(crate) fn session_board_enabled(&self) -> bool {
        crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(SESSION_BOARD_HIDDEN_SETTINGS_KEY)
            .and_then(Value::as_bool)
            != Some(true)
    }

    pub(crate) fn toggle_session_board(&mut self, cx: &mut gpui::Context<Self>) {
        if self.session_board_showing() {
            self.leave_session_board(cx);
        } else {
            self.show_session_board(cx);
        }
    }

    pub(crate) fn show_session_board(&mut self, cx: &mut gpui::Context<Self>) {
        if self.session_board_showing() || !self.session_board_enabled() {
            return;
        }
        self.native_session_board.showing = true;
        if self.native_session_board.restore_sidebar_collapsed.is_none() {
            self.native_session_board.restore_sidebar_collapsed = Some(self.sidebar_collapsed);
            if !self.sidebar_collapsed {
                self.toggle_gpui_sidebar_collapsed(cx);
            }
        }
        self.rebuild_native_session_board(cx);
        self.session_board_coverage_changed(cx);
    }

    pub(crate) fn leave_session_board(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.session_board_showing() {
            return;
        }
        self.native_session_board.showing = false;
        if let Some(collapsed) = self.native_session_board.restore_sidebar_collapsed.take()
            && self.sidebar_collapsed != collapsed
        {
            self.toggle_gpui_sidebar_collapsed(cx);
        }
        self.session_board_coverage_changed(cx);
        self.focus_default_surface_for_active_mode(cx);
    }

    /// The workarea appeared or vanished under the Board, so every terminal, chat and page child
    /// view that reads `agents_workspace_visible()` or the view gates is reconciled now, as a view
    /// switch reconciles them.
    fn session_board_coverage_changed(&mut self, cx: &mut gpui::Context<Self>) {
        self.reconcile_agents_pane_surfaces(cx);
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    /// A card's chat opens beside the lanes. Its thread is focused behind the Board, in its own
    /// project, because a chat lives in the Agents workspace; leaving the Board shows it there.
    pub(crate) fn session_board_open_chat(&mut self, card: BoardCard, cx: &mut gpui::Context<Self>) {
        let sidebar_session_id = card.sidebar_session_id.clone();
        self.set_session_board_chat(Some(card), cx);
        self.dispatch_native_sidebar_ui(
            json!({ "type": "selectSession", "mode": "focus", "sessionId": sidebar_session_id }),
            cx,
        );
    }

    pub(crate) fn session_board_close_chat(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_session_board_chat(None, cx);
    }

    fn set_session_board_chat(&mut self, card: Option<BoardCard>, cx: &mut gpui::Context<Self>) {
        let id = card.as_ref().map(|card| card.sidebar_session_id.clone());
        self.native_session_board.chat = card;
        if let Some(view) = self.native_session_board.view.clone() {
            view.update(cx, |view, cx| view.set_chat_card(id, cx));
        }
        self.reconcile_agents_pane_surfaces(cx);
        cx.notify();
    }

    /// A sidebar row clicked while the Board covers the workarea: a thread on the Board opens in its
    /// chat panel; anything else (a browser row, a thread outside the Space) leaves the Board so the
    /// focus the click makes is on screen.
    pub(crate) fn session_board_sidebar_focus(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.session_board_showing()
            || command["type"] != "selectSession"
            || command["mode"] != "focus"
        {
            return;
        }
        let Some(session_id) = command["sessionId"].as_str() else {
            return;
        };
        if self
            .native_session_board
            .chat
            .as_ref()
            .is_some_and(|card| card.sidebar_session_id == session_id)
        {
            return;
        }
        let card = self
            .native_session_board
            .view
            .as_ref()
            .and_then(|view| view.read(cx).card_by_id(session_id));
        match card {
            Some(card) if !card.task_waiting => self.set_session_board_chat(Some(card), cx),
            _ => {
                self.set_session_board_chat(None, cx);
                self.leave_session_board(cx);
            }
        }
    }

    /// Open in chats, from the chat panel: the Board gives way and the thread is where it was
    /// focused.
    pub(crate) fn session_board_open_in_chats(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_session_board_chat(None, cx);
        self.leave_session_board(cx);
    }

    /// The Board page, created and built on its first draw.
    ///
    /// CDXC:SessionBoard 2026-10-28 WHY: The page is a cached view like the Bot feed's, so terminal output and other app redraws do not rebuild every card; it redraws when its lanes change, its own clock ticks, or the window glass changes.
    fn session_board_page(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyView {
        let view = match self.native_session_board.view.clone() {
            Some(view) => view,
            None => {
                let app = cx.weak_entity();
                let view = cx.new(|cx| NativeSessionBoardView::new(app, cx));
                self.native_session_board.view = Some(view.clone());
                self.rebuild_native_session_board(cx);
                view
            }
        };
        if self.native_session_board.stale {
            self.rebuild_native_session_board(cx);
        }
        let glass = window_glass_active_in(window);
        if view.update(cx, |view, _| view.sync(glass)) {
            window.render_view_this_frame(view.entity_id());
        }
        AnyView::from(view)
    }

    /// The whole workspace column while the Board covers it: its own header row, then the lanes
    /// with the chat panel beside them.
    pub(crate) fn render_session_board_workarea(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let p = KanbanPalette::current(window);
        let page = self.session_board_page(window, cx);
        let chat = self
            .native_session_board
            .chat
            .clone()
            .map(|card| self.render_session_board_chat_panel(&card, &p, window, cx));
        v_flex()
            .id("ghostex-gpui-session-board-workarea")
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(p.page)
            .child(self.render_session_board_header(window, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .items_stretch()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(page.cached(StyleRefinement::default().size_full())),
                    )
                    .children(chat),
            )
            .into_any_element()
    }

    /// The header row over the Board: the window's drag area, the corner toggles while the sidebar
    /// is folded away (the docked sidebar's Search row draws them otherwise), and the caption
    /// buttons where the window draws its own.
    fn render_session_board_header(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let window_controls = (cfg!(target_os = "windows")
            || matches!(
                window.window_decorations(),
                gpui::Decorations::Client { .. }
            ))
        .then(|| {
            self.render_titlebar_window_controls(window, cx)
                .into_any_element()
        });
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let window_controls: Option<AnyElement> = {
            let _ = window;
            None
        };
        let leading_inset = if self.sidebar_collapsed {
            SIDEBAR_TOGGLE_LEADING_X
        } else {
            WORKAREA_HEADER_EDGE_PADDING
        };
        let row = h_flex()
            .id("ghostex-gpui-session-board-header")
            .flex_none()
            .w_full()
            .h(px(WORKAREA_HEADER_HEIGHT))
            .pl(px(leading_inset))
            .items_center()
            .when(self.sidebar_collapsed, |this| {
                this.child(
                    div()
                        .relative()
                        .top(px(0.5))
                        .child(self.render_sidebar_collapse_button(None, cx)),
                )
                .child(
                    div()
                        .relative()
                        .ml(px(4.0))
                        .top(px(0.5))
                        .child(self.render_workarea_header_agents_toggle(None, cx)),
                )
                .child(
                    div()
                        .relative()
                        .ml(px(4.0))
                        .top(px(0.5))
                        .child(self.render_workarea_header_board_toggle(None, cx)),
                )
            })
            .child(div().flex_1().h_full())
            .children(window_controls);
        window_drag_region(row)
    }

    /// A Board field took focus: the page is GPUI's own, so nothing else keeps the keys, as with
    /// the Kanban's fields (`native_kanban_input_focused`).
    pub(crate) fn session_board_input_focused(&mut self) {
        self.pending_keyboard_handoff = None;
        self.reclaim_gpui_root_for_chrome_input_focus();
    }

    /// The settings or theme the Board draws from changed.
    pub(crate) fn native_session_board_notify_appearance(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(view) = self.native_session_board.view.as_ref() {
            gpui::App::notify(cx, view.entity_id());
        }
    }
}

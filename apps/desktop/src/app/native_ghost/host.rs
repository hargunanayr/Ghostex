//! The app side of a Ghost's Overview: which Ghost it shows, the sidebar buttons that open and
//! close it, reading it afresh when it opens, and drawing the cached page in the view panel.

use super::view::NativeGhostOverviewView;
use crate::GhostexGpuiApp;
use crate::app::helpers::window_glass_active_in;
use crate::app::model::*;
use gpui::{
    AnyElement, AnyView, AppContext as _, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, StyleRefinement, Styled as _, Window, div,
};

impl GhostexGpuiApp {
    /// The Overview panel shows this Ghost right now.
    pub(crate) fn ghost_overview_showing(&self, ghost_id: &str) -> bool {
        self.active_mode == TitlebarMode::GhostOverview
            && self.ghost_overview_ghost_id.as_deref() == Some(ghost_id)
    }

    /// A Ghost row's Overview button: shows that Ghost's Overview beside its chat, or closes the
    /// panel when it already shows it (like the Automations row and the feed).
    pub(crate) fn toggle_ghost_overview(
        &mut self,
        ghost_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.ghost_overview_showing(ghost_id) {
            self.close_view_tab(TitlebarMode::GhostOverview, window, cx);
            return;
        }
        self.show_ghost_overview(ghost_id, window, cx);
    }

    /// Shows a Ghost's Overview, switching the panel to it when it showed another Ghost.
    pub(crate) fn show_ghost_overview(
        &mut self,
        ghost_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let changed = self.ghost_overview_ghost_id.as_deref() != Some(ghost_id);
        self.ghost_overview_ghost_id = Some(ghost_id.to_string());
        if let Some(view) = &self.native_ghost_overview {
            let ghost_id = ghost_id.to_string();
            view.update(cx, |view, cx| view.show_ghost(ghost_id, cx));
        }
        if self.active_mode == TitlebarMode::GhostOverview {
            if changed {
                cx.notify();
            }
            return;
        }
        self.open_view_tab(TitlebarMode::GhostOverview, window, cx);
    }

    /// Shows a Ghost's Overview from where no window is at hand: picking a Ghost (`open_ghost`)
    /// and the New Ghost modal's answer.
    pub(crate) fn show_ghost_overview_in_main_window(
        &mut self,
        ghost_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        let (app, ghost_id) = (cx.weak_entity(), ghost_id.to_string());
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| app.show_ghost_overview(&ghost_id, window, cx));
            });
        });
    }

    /// The Overview became the view showing (`set_active_mode`): read the Ghost afresh.
    pub(crate) fn native_ghost_overview_opened(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(view) = &self.native_ghost_overview {
            view.update(cx, |view, cx| view.load(cx));
        }
    }

    /// The view panel's Overview page, created (and loaded) on its first draw.
    pub(crate) fn render_native_ghost_overview_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let ghost_id = self.ghost_overview_ghost_id.clone();
        let view = match self.native_ghost_overview.clone() {
            Some(view) => view,
            None => {
                let app = cx.weak_entity();
                let view = cx.new(|cx| {
                    let mut view = NativeGhostOverviewView::new(app, ghost_id.clone(), cx);
                    view.load(cx);
                    view
                });
                self.native_ghost_overview = Some(view.clone());
                view
            }
        };
        let glass = window_glass_active_in(window);
        if view.update(cx, |view, _| view.sync(glass)) {
            window.render_view_this_frame(view.entity_id());
        }
        div()
            .id("ghostex-gpui-native-ghost-overview-host")
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.focus_project_editor_surface(TitlebarMode::GhostOverview, window, cx);
                }),
            )
            .child(AnyView::from(view).cached(StyleRefinement::default().size_full()))
            .into_any_element()
    }

    /// A Ghost changed (created, renamed, its files saved): the Overview reads it again.
    pub(crate) fn native_ghost_overview_reload(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(view) = &self.native_ghost_overview {
            view.update(cx, |view, cx| view.load(cx));
        }
    }

    /// Focuses a session from the Overview the way a sidebar row click does.
    pub(crate) fn focus_session_from_ghost_overview(
        &mut self,
        project_id: &str,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let sidebar_session_id = format!("combined-session:{project_id}:{session_id}");
        self.dispatch_native_sidebar_ui(
            serde_json::json!({"type": "selectSession", "sessionId": sidebar_session_id, "mode": "focus"}),
            cx,
        );
        let _ = self.react_to_native_sidebar_session_click(&sidebar_session_id, cx);
        self.reveal_floating_sessions(cx);
    }
}

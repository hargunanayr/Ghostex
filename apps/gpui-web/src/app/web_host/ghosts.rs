//! The Ghosts section's buttons in the browser: the Overview panel and the Ghost modal are
//! desktop-only, like the Bot feed. A Ghost row still opens the Ghost's chat here.

use gpui::{Context, Window};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn ghost_overview_showing(&self, _ghost_id: &str) -> bool {
        false
    }

    /// Picking a Ghost here opens only its chat.
    pub(crate) fn show_ghost_overview_in_main_window(
        &mut self,
        _ghost_id: &str,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(crate) fn toggle_ghost_overview(
        &mut self,
        _ghost_id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ghost_needs_the_app(cx);
    }

    pub(crate) fn open_new_ghost_modal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.ghost_needs_the_app(cx);
    }

    pub(crate) fn open_ghost_settings_modal(
        &mut self,
        _ghost_id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ghost_needs_the_app(cx);
    }

    fn ghost_needs_the_app(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not available in the browser",
            "Creating a Ghost and its Overview and settings need the Ghostex app (or ghostex ghost in a terminal).",
            cx,
        );
    }
}

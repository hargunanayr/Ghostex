//! The Unified header's search field.

use gpui::{AppContext as _, Entity, Subscription};
use gpui_component::input::{InputEvent, InputState};

use ghostex_gx_core::ChangeSummary;

use crate::GhostexGpuiApp;

pub(crate) struct UnifiedSearch {
    pub(crate) input: Entity<InputState>,
    _subscription: Subscription,
}

impl GhostexGpuiApp {
    /// The field's input, made the first time the header draws, and what it types into.
    pub(super) fn unified_search_input(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<InputState> {
        if let Some(search) = &self.native_sidebar.unified_search {
            return search.input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let subscription =
            cx.subscribe_in(&input, window, |app, input, event, _, cx| match event {
                InputEvent::Focus => {
                    app.drop_pending_browser_keyboard_handoff();
                    app.reclaim_gpui_root_for_chrome_input_focus();
                }
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    if app.gx_store.unified_sidebar.filter.query != query {
                        app.gx_store.unified_sidebar.filter.query = query;
                        app.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
                    }
                }
                _ => {}
            });
        self.native_sidebar.unified_search = Some(UnifiedSearch {
            input: input.clone(),
            _subscription: subscription,
        });
        input
    }
}

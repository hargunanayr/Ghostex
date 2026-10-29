// Closing web pages nobody is looking at, so the web runtime gives their memory back.

use crate::app::model::*;
use crate::cef::PageKeepAwake;
use crate::*;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How often hidden pages are checked. A page closes within one interval after its Auto Sleep time.
const WEB_PAGE_SLEEP_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// The pages the background Spaces would open on, by workspace project id.
#[derive(Default)]
pub(crate) struct SpaceKeptWebPages {
    browser_tabs: HashMap<String, HashSet<BrowserTabId>>,
    views: HashMap<String, TitlebarMode>,
}

impl SpaceKeptWebPages {
    pub(crate) fn keeps_view(&self, project_id: &str, mode: TitlebarMode) -> bool {
        self.views.get(project_id) == Some(&mode)
    }

    fn keeps_browser_tab(&self, project_id: &str, tab_id: BrowserTabId) -> bool {
        self.browser_tabs
            .get(project_id)
            .is_some_and(|tabs| tabs.contains(&tab_id))
    }
}

/// Whether a page has been off screen for `limit` and does not ask to stay awake. A page with no
/// fresh answer is asked now and looked at again on the next pass.
fn web_page_is_due(surface: &Entity<CefSurface>, limit: Duration, cx: &gpui::App) -> bool {
    let surface = surface.read(cx);
    if surface.hidden_for().is_none_or(|hidden| hidden < limit) {
        return false;
    }
    match surface.page_keep_awake() {
        PageKeepAwake::Release => true,
        PageKeepAwake::Keep => false,
        PageKeepAwake::Unknown => {
            surface.ask_page_keep_awake();
            false
        }
    }
}

impl GhostexGpuiApp {
    /// A running Claude cloud session's claude.ai page (see the 2026-10-23 sentence below).
    fn keeps_claude_cloud_page_awake(&self, url: &str) -> bool {
        ghostex_gx_core::is_running_claude_cloud_page(&self.gx_store.core, url)
    }

    /// CDXC:SessionSleep 2026-09-30 DECISION:
    /// User: "sleep the web runtime when we can if nothing is needing it so it stops taking ram", following the Auto Sleep settings. A page off screen for its Auto Sleep time is closed and its tab or view stays in place, reloading when selected: browser tabs of every project after Browser Auto Sleep, Files pages and website or extension views after Project Auto Sleep, and Off keeps them. Pages playing sound, pages whose last edited box still holds text (asked of the page, `cef/shell/page_keep_awake.rs`) and the page each background Space opens on stay awake, the last because the user wants swiping between Spaces "to show the web pages instant" (CDXC:Spaces 2026-09-30). The timers only marked views asleep and hid their pages, which kept every renderer running. VS Code keeps its own Auto Sleep, which also stops the code-server the other projects share. The runtime's own core cannot restart within one run, so it stays until the app quits.
    /// 2026-10-23: a Claude cloud session's claude.ai page also stays awake, but "only while the cloud session runs" (the user's answer), because the page is where Ghostex reads whether the session is working or needs the user (CDXC:ClaudeCloud in app/claude_cloud_page.rs); once the session is brought to this computer or fails, its page sleeps like any other.
    pub(crate) fn start_web_page_sleep_sweep(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(WEB_PAGE_SLEEP_SWEEP_INTERVAL)
                    .await;
                if this
                    .update(cx, |this, cx| this.sweep_hidden_web_pages(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn sweep_hidden_web_pages(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.cef_runtime_requested {
            return;
        }
        let space_kept = if self.has_parked_web_pages() {
            self.space_kept_web_pages()
        } else {
            SpaceKeptWebPages::default()
        };
        self.refresh_space_kept_parked_workarea_surfaces(&space_kept);
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let browser_limit =
            settings.auto_sleep_duration(shared_settings::SharedSettingsAutoSleepTarget::Browser);
        let view_limit = settings
            .auto_sleep_duration(shared_settings::SharedSettingsAutoSleepTarget::ProjectEditor);
        let mut slept = false;

        if let Some(limit) = browser_limit {
            let live_due = self
                .browser_surfaces
                .iter()
                .filter(|(tab_id, _)| {
                    !self
                        .browser_tabs
                        .tab(**tab_id)
                        .is_some_and(|tab| self.keeps_claude_cloud_page_awake(&tab.url))
                })
                .filter(|(_, surface)| web_page_is_due(surface, limit, cx))
                .map(|(tab_id, _)| *tab_id)
                .collect::<Vec<_>>();
            for tab_id in live_due {
                self.sleep_browser_tab(tab_id, cx);
                slept = true;
            }
            let parked_due = self
                .parked_browser_runtimes_by_project
                .iter()
                .flat_map(|(project_id, runtime)| {
                    runtime
                        .surfaces
                        .iter()
                        .filter(|(tab_id, surface)| {
                            !space_kept.keeps_browser_tab(project_id, **tab_id)
                                && !self
                                    .parked_browser_tabs_by_project
                                    .get(project_id)
                                    .and_then(|tabs| tabs.tab(**tab_id))
                                    .is_some_and(|tab| self.keeps_claude_cloud_page_awake(&tab.url))
                                && web_page_is_due(surface, limit, cx)
                        })
                        .map(|(tab_id, _)| (project_id.clone(), *tab_id))
                })
                .collect::<Vec<_>>();
            for (project_id, tab_id) in parked_due {
                self.sleep_parked_browser_tab(&project_id, tab_id, cx);
                slept = true;
            }
        }

        if let Some(limit) = view_limit {
            // The active view is left alone: its page would be rebuilt as soon as it closed.
            let live_due = self
                .project_workarea_runtime_cef_surfaces
                .iter()
                .filter(|(slot, owned)| {
                    **slot != ProjectWorkareaCefSurfaceSlotKey::Source
                        && slot.titlebar_mode() != self.active_mode
                        && web_page_is_due(&owned.surface, limit, cx)
                })
                .map(|(slot, _)| *slot)
                .collect::<Vec<_>>();
            let kept_awake = self.view_modes_kept_awake_by_parked_projects();
            for slot in live_due {
                let mode = slot.titlebar_mode();
                // A view another project's Space holds awake keeps its flag; only this page goes.
                if kept_awake.contains(&mode) {
                    self.remove_project_workarea_runtime_cef_surface(slot, cx);
                    self.update_active_mode_cef_child_visibility(cx);
                } else {
                    self.sleep_titlebar_view(mode, cx);
                }
                slept = true;
            }
            let parked_due = self
                .project_views
                .parked_views()
                .filter(|(_, project_id, mode, surface)| {
                    !space_kept.keeps_view(project_id, *mode) && web_page_is_due(surface, limit, cx)
                })
                .map(|(key, ..)| key.to_owned())
                .collect::<Vec<_>>();
            for key in parked_due {
                slept |= self.project_views.close_parked_view(&key);
            }
        }

        if slept {
            cx.notify();
        }
    }

    fn has_parked_web_pages(&self) -> bool {
        self.parked_browser_surface_count() > 0
            || !self.parked_project_workarea_surfaces.is_empty()
            || self.project_views.parked_views().next().is_some()
    }

    /// The view each background Space opens on, when it is a web page: the browser tabs drawn in
    /// that project's panes, or its Files, website or extension view.
    fn space_kept_web_pages(&self) -> SpaceKeptWebPages {
        let mut kept = SpaceKeptWebPages::default();
        for project_id in self.gx_store_space_landing_project_ids() {
            if self.agents_workspace_project_id.as_deref() == Some(project_id.as_str()) {
                continue;
            }
            let Some(state) = self.project_view_states_by_project.get(&project_id) else {
                continue;
            };
            match state.active_mode {
                TitlebarMode::Browser => {
                    if let Some(tabs) = self.parked_browser_tabs_by_project.get(&project_id) {
                        kept.browser_tabs
                            .insert(project_id, tabs.rendered_active_loaded_tab_ids());
                    }
                }
                TitlebarMode::Agents
                | TitlebarMode::Terminal
                | TitlebarMode::BotFeed | TitlebarMode::GhostOverview
                | TitlebarMode::Work => {}
                mode => {
                    kept.views.insert(project_id, mode);
                }
            }
        }
        kept
    }
}

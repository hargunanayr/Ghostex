//! The Sidebar group of General: Sidebar, Session Cards and Sidebar Tags.
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::super::fields::{
    RowSpec, card_inset, custom_note, hover_actions_field, hover_buttons_are_default,
    normalize_hover_buttons, reset_key, segmented_field, settings_section, tag_list_field,
    toggle_field_with,
};
use super::super::super::page::PageBlock;
use super::super::super::store::SettingsStore;
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context, SharedString, Window};
use serde_json::{Map, Value, json};
use std::rc::Rc;

/// `SIDEBAR_SETTINGS_PRESETS` as `(id, label, settings)`.
fn presets() -> Vec<(String, String, Map<String, Value>)> {
    settings_catalog()
        .module_value(module::SETTINGS, "SIDEBAR_SETTINGS_PRESETS")
        .and_then(Value::as_array)
        .map(|presets| {
            presets
                .iter()
                .filter_map(|preset| {
                    Some((
                        preset.get("id")?.as_str()?.to_string(),
                        preset.get("label")?.as_str()?.to_string(),
                        preset.get("settings")?.as_object()?.clone(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `getSidebarSettingsPresetId`: the preset whose every controlled key matches, if any.
fn active_preset(g: &GeneralCx) -> Option<String> {
    let keys =
        settings_catalog().string_list(module::SETTINGS, "SIDEBAR_SETTINGS_PRESET_KEYS", None);
    presets().into_iter().find_map(|(id, _, settings)| {
        keys.iter()
            .all(|key| {
                settings
                    .get(key)
                    .is_some_and(|value| *value == g.values.value(key))
            })
            .then_some(id)
    })
}

/// `updateSidebarSettingsPreset(presetId)`: the whole draft with the preset's keys applied.
fn apply_preset(page: &mut GeneralTab, preset_id: &str, cx: &mut Context<GeneralTab>) {
    let Some((_, _, preset)) = presets().into_iter().find(|(id, _, _)| id == preset_id) else {
        return;
    };
    let store = page.store.clone();
    store.update(cx, |store: &mut SettingsStore, cx| {
        let mut settings = store.settings().clone();
        for (key, value) in preset {
            settings.insert(key, value);
        }
        store.apply_settings(settings, "settings:bulk", cx);
    });
}

fn sidebar_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sidebar", g.power_visible) {
        return None;
    }
    let s = "sidebar";
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:Settings 2026-06-12-07:10: Preset leads the Sidebar section.
    if g.visible(s, "sidebarSettingsPreset") {
        let active = active_preset(g);
        let options: Vec<SettingOption> = presets()
            .into_iter()
            .map(|(id, label, _)| SettingOption { label, value: id })
            .collect();
        let spec = RowSpec::new("Preset")
            .description("Apply a sidebar UI preset.")
            .modified(active.as_deref() != Some("recommended"));
        rows.push(segmented_field(
            &g.p,
            "sidebarSettingsPreset",
            spec,
            Some(Rc::new(
                |page: &mut GeneralTab, _window, cx: &mut Context<GeneralTab>| {
                    apply_preset(page, "recommended", cx)
                },
            )),
            &options,
            active.as_deref(),
            active.is_none().then(|| custom_note(&g.p)),
            |page: &mut GeneralTab, next, _window, cx| apply_preset(page, &next, cx),
            cx,
        ));
    }
    if g.visible(s, "sidebarLayout") {
        let spec = g.spec(
            "sidebarLayout",
            "Sidebar layout",
            "Classic lists every project with its sessions under it. Two columns puts your projects and the Needs you, Running and All sessions views on the left, and the selected project's sessions on the right, grouped by what needs you, what is running and when you last used them. Widen the sidebar to see both columns in full.",
        );
        rows.extend(page.segmented(
            g,
            s,
            "sidebarLayout",
            spec,
            Some(reset_key::<GeneralTab>("sidebarLayout")),
            g.options("SIDEBAR_LAYOUT_OPTIONS"),
            cx,
        ));
    }
    // CDXC:Settings 2026-09-11 DECISION: dependent rows are hidden, not disabled, while their parent is off.
    // Their parent is the Spaces switch on the Extensions page now (built_in_extensions.rs), so they
    // sit as plain rows rather than indented under a row this page no longer has.
    if ghostex_settings_catalog::built_in_extensions::enabled_with(
        ghostex_settings_catalog::built_in_extensions::SPACES,
        |key| Some(g.values.bool(key)),
    ) {
        rows.extend(page.select(
            g,
            s,
            "sidebarSpaceSwitchBehavior",
            "When switching to a Space",
            "Reopen the session you last had open in a Space when you switch to it, in the view its project was in. If that session is closed, the one before it is used; a Space with nothing remembered opens its first project.",
            g.options("SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS"),
            Some(256.0),
            false,
            window,
            cx,
        ));
        rows.extend(page.toggle(
            g,
            s,
            "sidebarSpaceFollowActiveSession",
            "Follow the active session's Space",
            "Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward, Search by Prompt, a notification, or Previous Sessions.",
            false,
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "projectSwitchKeepAliveMinutes",
        "Keep the previous project live for (minutes)",
        "After you switch to another project or Space, keep the terminals, chats, and view that were open in the previous project running for this many minutes so switching back is instant. 0 releases them right away.",
        (
            g.number("MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES"),
            g.number("MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES"),
            1.0,
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "sidebarVisibilityMemory",
        "Sidebar visibility memory",
        "Keep one sidebar state everywhere, or remember it separately for a window with no view open and a window with one open. The Commands pane always follows that same distinction.",
        g.options("SIDEBAR_VISIBILITY_MEMORY_OPTIONS"),
        Some(256.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(g, s, "showProjectIcons", "Show project icons", "Show project artwork or a square with the project\u{2019}s first letter beside project names.", false, cx));
    rows.extend(page.toggle(
        g,
        s,
        "hideSessionAgentIconUntilHover",
        "Hide agent icon until hover",
        "Hide session agent icons until a session row is hovered.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "hideBrowserFaviconUntilHover",
        "Hide browser favicon until hover",
        "Hide browser page favicons until a session row is hovered.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "hideLastActiveTimeOnSessionCards",
        "Hide last active time",
        "Hide Last Active timestamps from session-card title rows.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "highlightPendingQuestions",
        "Highlight unanswered questions",
        "Give sessions with a detected unanswered question a soft pink background, including while the agent keeps working. Currently supports Codex asynchronous questions.",
        false,
        cx,
    ));
    // CDXC:SessionStatus 2026-10-15 DECISION: User (Q120, supersedes Q112's Auto): Ghostex detection, Combined, or Program status only (Combined is stored as `auto`); gxserver reads it (server/src/program_status/mode.rs) so the desktop, web, phone and CLI all follow it.
    rows.extend(page.select(
        g,
        s,
        "sessionStatusSource",
        "Session status from",
        "Choose how Ghostex knows a session is working, needs you or is done. Ghostex detection reads each agent's hooks, terminal title and screen. Combined keeps that detection and adds the status a program reports about itself (OSC 7501, sent by Claude Code 2.1.295 and newer, recent Codex betas and your own scripts): failures, what it is waiting for, its message and progress on hover, and waiting or finished the moment the program says so. Program status only uses the program's own status alone.",
        g.options("SESSION_STATUS_SOURCE_OPTIONS"),
        Some(256.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "hideProjectHeaderDiffStats",
        "Hide project git stats",
        "Hide +added/-removed line counts in sidebar project rows.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "showProjectEditorDiffFileCount",
        "Show changed-file count",
        "Show changed-file counts in sidebar project row git stats.",
        false,
        cx,
    ));
    if g.visible(s, "hideMenuBarSessionStatusIndicators") {
        // The row shows the inverse of the stored "hide" flag.
        let spec = g.spec(
            "hideMenuBarSessionStatusIndicators",
            "Show Menu Bar Session Indicators",
            "Show the menu bar session status badges.",
        );
        rows.push(toggle_field_with(
            &g.p,
            "hideMenuBarSessionStatusIndicators",
            spec,
            !g.values.bool("hideMenuBarSessionStatusIndicators"),
            Some(reset_key::<GeneralTab>(
                "hideMenuBarSessionStatusIndicators",
            )),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(
                    page,
                    "hideMenuBarSessionStatusIndicators",
                    json!(!checked),
                    cx,
                )
            },
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "sidebarDefaultWidthPx",
        "Default Width",
        "Used when double-clicking the sidebar resize handle. App restart still restores your last manually set sidebar width.",
        (g.number("MIN_SIDEBAR_DEFAULT_WIDTH_PX"), g.number("MAX_SIDEBAR_DEFAULT_WIDTH_PX"), 1.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "sidebarCollapseAnimationDurationMs",
        "Collapse Animation Duration",
        "Duration in milliseconds for expanding and collapsing sidebar sections, groups, and projects, and for the floating sidebar and Agents Panel sliding in from the window edge. Set to 0 for instant changes.",
        (
            g.number("MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS"),
            g.number("MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS"),
            g.number("SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS"),
        ),
        false,
        window,
        cx,
    ));
    if g.visible(s, "panelAnimationSpeed") {
        let spec = g.spec(
            "panelAnimationSpeed",
            "Panel Animations",
            "How fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed. Reduce Motion in your computer settings always turns it off.",
        );
        rows.extend(page.segmented(
            g,
            s,
            "panelAnimationSpeed",
            spec,
            Some(reset_key::<GeneralTab>("panelAnimationSpeed")),
            g.options("PANEL_ANIMATION_SPEED_OPTIONS"),
            cx,
        ));
    }
    rows.extend(page.toggle(g, s, "closeSidePanelWithLastTab", "Close side panel with its last tab", "Close the side panel when you close its last tab, instead of showing the Open a view picker.", false, cx));
    rows.extend(page.slider(
        g,
        s,
        "sidebarTooltipDelayMs",
        "Tooltip Delay",
        "Delay in milliseconds before sidebar tooltips appear. Set to 0 to show them immediately.",
        (
            g.number("MIN_SIDEBAR_TOOLTIP_DELAY_MS"),
            g.number("MAX_SIDEBAR_TOOLTIP_DELAY_MS"),
            g.number("SIDEBAR_TOOLTIP_DELAY_STEP_MS"),
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "commandsPanelDefaultHeightPx",
        "Command Pane Default Height",
        "Used when opening the command pane (F12 or sidebar) and when double-clicking its top resize rail.",
        (
            g.number("MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            g.number("MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            1.0,
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "commandsPanelSide",
        "Command Pane Side",
        "Where terminal Actions and F12 open the command pane: below the workspace or as a column to its right.",
        g.options("COMMANDS_PANEL_SIDE_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(g, s, "commandsPanelAutoMinimize", "Auto-minimize Commands pane", "Minimize the Commands pane after you stop using it and move focus elsewhere. Commands keep running.", false, cx));
    if g.values.bool("commandsPanelAutoMinimize") {
        rows.extend(page.number_select(
            g,
            s,
            "commandsPanelAutoMinimizeDelaySeconds",
            "Minimize after",
            "How long the Commands pane stays open after focus and the pointer leave it.",
            g.options("COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS"),
            true,
            window,
            cx,
        ));
    }
    // The tab strip's own buttons moved here from the Actions page, which hides while the
    // Actions extension is off (built_in_extensions.rs).
    rows.extend(page.toggle(
        g,
        s,
        "hideTabStripNewTerminalButton",
        "Hide New Terminal button",
        "Hide the New Terminal button from the tab strip.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "hideTabStripNewBrowserButton",
        "Hide New Browser Tab button",
        "Hide the New Browser Tab button from the tab strip.",
        false,
        cx,
    ));
    // The sidebar's Sort & Filter menu writes the same setting (CDXC:Sidebar 2026-10-05 in gx-core
    // sidebar_view/ordering.rs `is_grouped_working`).
    rows.extend(page.toggle(
        g,
        s,
        "groupWorkingSessions",
        "Group working sessions",
        "Move sessions into a collapsed Working section under their project while their agent works; they return to Sessions when it stops or needs you. Also in the sidebar's Sort & Filter menu.",
        false,
        cx,
    ));
    // CDXC:Sidebar 2026-06-16-18:19: agentManagerZoomPercent is labelled Sidebar Interface Size.
    rows.extend(page.slider(
        g,
        s,
        "agentManagerZoomPercent",
        "Sidebar Interface Size",
        "Scale the sidebar interface.",
        (50.0, 200.0, 1.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "createSessionOnSidebarDoubleClick",
        "Double-click empty sidebar space to create a session",
        "Create a session from empty sidebar space.",
        false,
        cx,
    ));
    rows.extend(page.toggle(g, s, "enableSessionParking", "Enable session parking", "Add Park to session menus and move parked sessions into a collapsible section at the bottom of the sidebar.", false, cx));
    if g.values.bool("enableSessionParking") {
        rows.extend(page.toggle(
            g,
            s,
            "sleepSessionWhenParking",
            "Sleep session when parking",
            "Sleep a session through its normal lifecycle immediately after it is parked.",
            true,
            cx,
        ));
    }
    // CDXC:Settings 2026-09-12 DECISION: Park & Snooze with tags has the ↳ prefix and stays available while parking is off.
    rows.extend(page.toggle(
        g,
        s,
        "showTagMenuWhenParking",
        "Park & Snooze with tags",
        "Open the Tag as menu when a session is parked or snoozed so it can be tagged right away.",
        true,
        cx,
    ));
    if g.values.bool("enableSessionParking") {
        rows.extend(page.toggle(g, s, "unparkAfterSendingMessage", "Unpark after sending a message", "Move a parked session out of the Parked section when you send it a message from chat or its terminal.", true, cx));
    }
    if g.visible(s, "renameSessionOnDoubleClick") {
        let label = settings_catalog().text(
            module::SETTINGS_TYPES,
            "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL",
        );
        let subtitle = settings_catalog().text(
            module::SETTINGS_TYPES,
            "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE",
        );
        let spec = g
            .spec(
                "renameSessionOnDoubleClick",
                &label,
                "Rename sessions directly from their cards.",
            )
            .subtitle(subtitle);
        rows.push(toggle_field_with(
            &g.p,
            "renameSessionOnDoubleClick",
            spec,
            g.values.bool("renameSessionOnDoubleClick"),
            Some(reset_key::<GeneralTab>("renameSessionOnDoubleClick")),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "renameSessionOnDoubleClick", json!(checked), cx)
            },
            cx,
        ));
    }
    settings_section(&g.p, "Sidebar", None, None, rows)
        .map(|section| PageBlock::section("sidebar", section))
}

fn session_cards_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sessionCards", g.power_visible) {
        return None;
    }
    let s = "sessionCards";
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(s, "sessionCardHoverButtons") {
        let value = g.values.value("sessionCardHoverButtons");
        let items = normalize_hover_buttons(&value);
        let spec = g
            .spec(
                "sessionCardHoverButtons",
                "Session hover buttons (click to toggle, drag to reorder)",
                "Buttons a session card shows when you hover it. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default the strip is Tag, Park, Sleep, chevron, Close.",
            )
            .modified(!hover_buttons_are_default(&items));
        rows.push(hover_actions_field(
            page,
            &g.p,
            spec,
            Some(reset_key::<GeneralTab>("sessionCardHoverButtons")),
            &value,
            cx,
        ));
    }
    rows.extend(page.select(
        g,
        s,
        "sidebarSessionLinks",
        "Session links in the sidebar",
        "How a session card shows the pull requests, issues and Linear tickets it is linked to. Always: a line of them under every card that has any. Selected session: that line only under the session you have open. Badge: a small count on the card, coloured by the most pressing one. Open work only: a line of the ones still open, none once they are all merged or closed. Hovering a card with links shows them all in a card beside the sidebar, except with Off, which shows nothing.",
        g.options("SIDEBAR_SESSION_LINKS_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "titleSessionsByBranch",
        "Title sessions by their branch",
        "Name a session after the branch it works on, without your name and the ticket number (yahia/spx-1245-copy-link shows as copy-link), until you rename it yourself. Sessions on main keep their own title.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "offerCleanupWhenPrMerges",
        "Offer Clean up when a PR merges",
        "When a session's pull request is merged, its card asks once whether to clean up (remove the session's worktree and park the session; a worktree with uncommitted changes is kept) or keep it.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "showSessionCardHoverButtonsInContextMenu",
        "Hover buttons also in context menu",
        "Also list the enabled hover buttons in the session right-click menu, in their usual place in the menu. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card.",
        false,
        cx,
    ));
    settings_section(&g.p, "Session Cards", None, None, rows)
        .map(|section| PageBlock::section("sessionCards", section))
}

fn sidebar_tags_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sidebarTags", g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible("sidebarTags", "sidebarSessionTagListItems") {
        let value = g.values.value("sidebarSessionTagListItems");
        rows.push(card_inset(tag_list_field(
            page,
            &g.p,
            "Reorder, hide, or disable sidebar tag filters and separators.",
            &value,
            g.create_tag,
            reset_key::<GeneralTab>("sidebarSessionTagListItems"),
            window,
            cx,
        )));
    }
    settings_section(&g.p, SharedString::from("Sidebar Tags"), None, None, rows)
        .map(|section| PageBlock::section("sidebarTags", section))
}

pub(super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        sidebar_section(page, g, window, cx),
        session_cards_section(page, g, window, cx),
        sidebar_tags_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

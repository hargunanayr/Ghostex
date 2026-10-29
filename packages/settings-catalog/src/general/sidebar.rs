use crate::data::*;
use crate::json::opt;
use crate::json::J;
use crate::rows::{option, row, section, Section, SettingOption};

pub(crate) fn session_cards() -> Section {
    section(
        "sessionCards",
        "Session Cards",
        vec![
            // CDXC:Sessions 2026-05-15-19:46:
            // Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
            row("sessionCardHoverButtons", "Session hover buttons (click to toggle, drag to reorder)", "Buttons a session card shows when you hover it. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default the strip is Tag, Park, Sleep, chevron, Close."),
            row("sidebarSessionLinks", "Session links in the sidebar", "How a session card shows the pull requests, issues and Linear tickets it is linked to. Always: a line of them under every card that has any. Selected session: that line only under the session you have open. Badge: a small count on the card, coloured by the most pressing one. Open work only: a line of the ones still open, none once they are all merged or closed. Hovering a card with links shows them all in a card beside the sidebar, except with Off, which shows nothing.").options(SIDEBAR_SESSION_LINKS_OPTIONS),
            row("titleSessionsByBranch", "Title sessions by their branch", "Name a session after the branch it works on, without your name and the ticket number (yahia/spx-1245-copy-link shows as copy-link), until you rename it yourself. Sessions on main keep their own title."),
            row("offerCleanupWhenPrMerges", "Offer Clean up when a PR merges", "When a session's pull request is merged, its card asks once whether to clean up (remove the session's worktree and park the session; a worktree with uncommitted changes is kept) or keep it."),
            row("showSessionCardHoverButtonsInContextMenu", "Hover buttons also in context menu", "Also list the enabled hover buttons in the session right-click menu, in their usual place in the menu. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card."),
        ],
    )
}

pub(crate) fn sidebar() -> Section {
    section(
        "sidebar",
        "Sidebar",
        vec![
            row("sidebarSettingsPreset", "Preset", "Apply a sidebar UI preset or show Custom when controlled settings diverge.").options_of(SIDEBAR_SETTINGS_PRESETS, "label", "id").options(&[opt("Custom", "custom")]),
            row("sidebarLayout", "Sidebar layout", "Classic lists every project with its sessions under it. Two columns puts your projects and the Needs you, Running and All sessions views on the left, and the selected project's sessions on the right, grouped into Needs you, Running, Today, This week and Older, with a filter for Needs you or Running. Widen the sidebar to see both columns in full.").options(SIDEBAR_LAYOUT_OPTIONS),
            row("sidebarSpaceSwitchBehavior", "When switching to a Space", "Reopen the session you last had open in a Space when you switch to it, in the view its project was in. Shown while the Spaces extension is on.").options(SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS),
            row("sidebarSpaceFollowActiveSession", "Follow the active session's Space", "Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward or Search by Prompt. Shown while the Spaces extension is on."),
            row("projectSwitchKeepAliveMinutes", "Keep the previous project live for", "After you switch to another project or Space, keep the terminals, chats, and view that were open in the previous project running for this many minutes so switching back is instant. 0 releases them right away."),
            row("sidebarVisibilityMemory", "Sidebar visibility memory", "Keep one sidebar state everywhere, or remember it separately for Agents and for the wide views (Browser, Code, Files, Kanban, Automate).").options(SIDEBAR_VISIBILITY_MEMORY_OPTIONS),
            row("showProjectIcons", "Show project icons", "Show project artwork or a square with the project’s first letter beside project names."),
            // CDXC:Settings 2026-06-30-22:22:
            // Search metadata follows the visible row order: preset-controlled rows
            // sit immediately after Preset, before independent sidebar sizing and
            // placement controls.
            row("hideSessionAgentIconUntilHover", "Hide agent icon until hover", "Hide session agent icons until a session row is hovered."),
            row("hideBrowserFaviconUntilHover", "Hide browser favicon until hover", "Hide browser page favicons until a session row is hovered."),
            row("hideLastActiveTimeOnSessionCards", "Hide last active time", "Hide Last Active timestamps from session-card title rows."),
            row("highlightPendingQuestions", "Highlight unanswered questions", "Give sessions with a detected unanswered question a soft pink background, including while the agent keeps working. Currently supports Codex asynchronous questions."),
            row("sessionStatusSource", "Session status from", "Choose how Ghostex knows a session is working, needs you or is done. Ghostex detection reads each agent's hooks, terminal title and screen. Combined keeps that detection and adds the status a program reports about itself (OSC 7501, sent by Claude Code 2.1.295 and newer, recent Codex betas and your own scripts): failures, what it is waiting for, its message and progress on hover, and waiting or finished the moment the program says so. Program status only uses the program's own status alone.").options(SESSION_STATUS_SOURCE_OPTIONS),
            row("hideProjectHeaderDiffStats", "Hide project git stats", "Hide +added/-removed line counts in sidebar project rows."),
            row("showProjectEditorDiffFileCount", "Show changed-file count", "Show changed-file counts in sidebar project row git stats."),
            row("hideMenuBarSessionStatusIndicators", "Show Menu Bar Session Indicators", "Show the menu bar session status badges."),
            row("sidebarCollapseAnimationDurationMs", "Collapse animation speed", "Set how quickly sidebar sections, groups, and projects expand or collapse, and how quickly the floating sidebar and Agents Panel slide in from the window edge. Set to 0 for no animation."),
            row("panelAnimationSpeed", "Panel animations", "Set how fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed: Off, Slow, Normal or Fast. Reduce Motion in your computer settings always turns it off.").options(PANEL_ANIMATION_SPEED_OPTIONS),
            row("closeSidePanelWithLastTab", "Close side panel with its last tab", "Close the side panel when you close its last tab, instead of showing the Open a view picker."),
            row("sidebarTooltipDelayMs", "Tooltip Delay", "Set how long sidebar hover labels wait before appearing. Set to 0 to show them immediately."),
            row("sidebarDefaultWidthPx", "Default Width", "Width restored when double-clicking the sidebar resize handle."),
            row("commandsPanelDefaultHeightPx", "Command Pane Default Height", "Height used when opening the command pane and when double-clicking its top resize rail."),
            row("commandsPanelSide", "Command Pane Side", "Dock the command pane below the workspace or to its right.").options(COMMANDS_PANEL_SIDE_OPTIONS),
            row("commandsPanelAutoMinimize", "Auto-minimize Commands pane", "Minimize the Commands pane after you stop using it and move focus elsewhere. Commands keep running."),
            row("commandsPanelAutoMinimizeDelaySeconds", "Minimize after", "How long the Commands pane stays open after focus and the pointer leave it.").num_options(COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS),
            row("hideTabStripNewTerminalButton", "Hide New Terminal button", "Hide the New Terminal button from the tab strip."),
            row("hideTabStripNewBrowserButton", "Hide New Browser Tab button", "Hide the New Browser Tab button from the tab strip."),
            row("groupWorkingSessions", "Group working sessions", "Move sessions into a collapsed Working section under their project while their agent works; they return to Sessions when it stops or needs you. Also in the sidebar's Sort & Filter menu."),
            row("agentManagerZoomPercent", "Sidebar Interface Size", "Scale the sidebar interface."),
            row("createSessionOnSidebarDoubleClick", "Double-click empty sidebar space to create a session", "Create a session from empty sidebar space."),
            row("enableSessionParking", "Enable session parking", "Move deferred sessions into a collapsible Parked section at the bottom of the sidebar."),
            row("sleepSessionWhenParking", "Sleep session when parking", "Sleep a session automatically when it is moved into the Parked section."),
            row("showTagMenuWhenParking", "Park & Snooze with tags", "Open the Tag as menu when a session is parked or snoozed so it can be tagged right away."),
            row("unparkAfterSendingMessage", "Unpark after sending a message", "Move a parked session out of the Parked section when you send it a message."),
            row("renameSessionOnDoubleClick", "Double-click session cards to rename", "Makes clicking on a session respond a bit slower so we can detect the double click"),
        ],
    )
}

pub(crate) fn sidebar_tags() -> Section {
    section(
        "sidebarTags",
        "Sidebar Tags",
        vec![
            row("sidebarSessionTagListItems", "Tag Filter List", "Add your own tags, then reorder, hide, disable, or delete tags and their separators for the sidebar and the Tag as menu.").option_list(tag_list_item_options()).options(&[opt("Hide tag", "hide"), opt("Disable tag", "disable"), opt("Reorder tags", "reorder")]),
        ],
    )
}

/// `getSidebarSessionTagListItemLabel` over the default tag list: a tag's label, "No tag", or "Separator".
fn tag_list_item_options() -> Vec<SettingOption> {
    DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS
        .as_array()
        .iter()
        .map(|item| {
            let id = item.get("id").and_then(J::as_str).unwrap_or_default();
            let label = match item.get("type").and_then(J::as_str) {
                Some("tag") => {
                    let tag = item.get("tag").and_then(J::as_str).unwrap_or_default();
                    SIDEBAR_SESSION_TAG_OPTIONS
                        .iter()
                        .find(|option| option.value == tag)
                        .map_or(tag, |option| option.label)
                }
                Some("untagged") => "No tag",
                _ => "Separator",
            };
            option(label, id)
        })
        .collect()
}

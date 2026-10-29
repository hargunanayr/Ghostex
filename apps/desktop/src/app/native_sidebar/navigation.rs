use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde_json::json;

use super::appearance::SidebarAppearance;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::render::window_drag_region::window_drag_region;
use crate::*;

/// The narrowest a top-row button gets when the sidebar is too narrow for the full 34pt slots:
/// its 15pt icon plus a little room, so every button stays visible.
pub(crate) const SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH: f32 = 20.0;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-19 DECISION:
    /// User: the Search row and the Commands row are each one pixel taller.
    /// Both rows keep those heights now that the hairlines are gone; the pixel each border used to
    /// take out of the border-box went back into the padding, so neither row's content moved.
    pub(crate) fn render_native_sidebar_navigation(
        &self,
        appearance: &SidebarAppearance,
        footer: bool,
        usage_meter_count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = if footer { "Commands" } else { "Search" };
        let action_id = if footer { "commands" } else { "sessions" };
        let snapshot = self
            .native_sidebar
            .snapshot
            .as_ref()
            .expect("navigation follows a snapshot");
        let shortcut = if footer {
            &snapshot.commands_shortcut
        } else {
            &snapshot.search_shortcut
        };
        // The menu is read when the button is pressed; sharing the snapshot keeps this row from
        // copying the menu's JSON on every frame.
        let more_menu_snapshot = snapshot.clone();
        /*
        CDXC:Sidebar 2026-09-20 DECISION:
        User: with the titlebar row deleted, the sidebar's Search row is what sits in the window's
        top-left corner, so it reserves the macOS traffic lights and is the window's drag handle
        there. It does so only while the sidebar is docked: the hover-reveal panel renders this same
        row below the workarea header, where there are no lights to clear.
        CDXC:Sidebar 2026-09-23 DECISION:
        User: on Windows and Linux too, the Toggle sidebar and Agents Panel buttons sit at the top
        left of the sidebar, and dragging the sidebar's top row moves the window. So this row owns
        the corner on every OS; only the traffic-light reserve is macOS's own
        (`SIDEBAR_TOGGLE_LEADING_X`). This supersedes the 2026-09-20 rule that kept the row a plain
        row off macOS, with both toggles in the workarea header.
        CDXC:Sidebar 2026-09-23 WHY:
        On Windows WM_NCHITTEST reports the Drag region as caption wherever no interactive child
        occludes it, and a caption press never reaches an `on_click`, so every button in this row
        occludes there.
        */
        let owns_window_corner = !footer && !self.sidebar_collapsed;
        /*
        CDXC:Sidebar 2026-09-21 DECISION:
        User: the Search and the Commands buttons are icon-only buttons with the icon they already
        show, the label and shortcut in the tooltip ("Search (⌘P)", "Commands (⌘⇧P)"), and the
        buttons of both rows align right, against the sidebar's trailing edge. This supersedes the
        2026-09-20 rule that aligned them left.
        CDXC:Sidebar 2026-10-11 DECISION:
        User: the bottom-left Commands item is always just the lightning icon button, aligned to the
        right of the sidebar's bottom bar with its "Commands (Ctrl+Shift+P)" tooltip, and never
        expands to the "Commands" label when the sidebar is wide. This supersedes the 2026-09-28
        width threshold and its 2026-10-11 441px revision.
        */
        /*
        CDXC:Sidebar 2026-10-11 DECISION:
        User: "i actually want search and notifications to be always shown, there's space for those
        too in all cases" (2026-10-05). Search, the bell and the menu button stay in the top row at
        every sidebar width, and nothing collapses into the sidebar menu any more. Send Bug / Feedback is a
        row of the More options menu (CDXC:Feedback 2026-10-14 in
        packages/gx-core/src/sidebar_menu/navigation.rs). This supersedes the 2026-09-23 rule that
        moved Search and Notifications into the menu together.
        */
        /*
        CDXC:Sidebar 2026-09-21 DECISION:
        User: the Toggle sidebar button sits left of Search, so it stays in exactly the same spot
        always. While the sidebar is docked this row owns the window's top-left corner, so it draws
        the button (right after the traffic lights on macOS) at the unscaled x the collapsed
        workarea header draws it (`SIDEBAR_TOGGLE_LEADING_X`); the header only draws it while
        collapsed.
        */
        // With Spaces on, the Hermes button is the Space row's last slot instead.
        let bots_toggle = (!footer && snapshot.bots_enabled && !snapshot.spaces_enabled)
            .then_some(snapshot.bots_mode);
        let bell_visible = !footer && self.titlebar_notification_bell_visible();
        let work_visible = !footer && !snapshot.work_mode_project_ids().is_empty();
        let icon_path = if footer {
            "titlebar/bolt.svg"
        } else {
            BROWSER_ICON_SEARCH
        };
        let tooltip_label: gpui::SharedString = match shortcut.as_deref() {
            Some(shortcut) if !shortcut.is_empty() => format!("{label} ({shortcut})").into(),
            _ => label.into(),
        };
        let tooltip_delay = appearance.tooltip_delay;
        h_flex()
            .w_full()
            .h(px((if footer { 39.0 } else { 35.0 }) * scale))
            .pt(px(5.0 * scale))
            .pb(px((if footer { 6.0 } else { 3.0 }) * scale))
            .px(px(5.0 * scale))
            .gap(px(4.0 * scale))
            /*
            CDXC:Sidebar 2026-10-24 WHY:
            GitHub #288: a right-aligned row that is wider than the sidebar overflows at its start,
            which slid the Toggle sidebar button left by the shortfall and under the macOS green
            window button (about 37pt at a 241pt sidebar with Search, the bell and the menu). The
            corner row therefore starts at the left, its spacer right-aligns the buttons, and the
            buttons right of the spacer narrow to `SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH` before
            anything overflows, so the toggle keeps its reserved x at every width.
            */
            .when(!owns_window_corner, |row| row.justify_end())
            .when(owns_window_corner, |row| {
                window_drag_region(row.pl(px(SIDEBAR_TOGGLE_LEADING_X)))
            })
            .overflow_hidden()
            /*
            CDXC:Sidebar 2026-09-20 DECISION:
            User: there is no rule under the Search row and none above the usage strip or the
            Commands row. The session list fades out at its bottom end only
            (native_sidebar/scroll_fade.rs) and nothing shades its top, so these rows draw no border
            at all. This supersedes the 2026-09-19 rule that framed the list with a hairline at each
            end.
            */
            .flex_shrink_0()
            .text_color(titlebar_active_text_color().opacity(0.52))
            /*
            CDXC:Sidebar 2026-09-23 DECISION:
            User: the Toggle sidebar and Agents Panel buttons at the top of the sidebar have the
            same color as the notification bell there, so this row draws their icons in the
            sidebar's muted color rather than the header's bright one.
            */
            .when(owns_window_corner, |row| {
                // The row's unscaled leading padding puts the button on the header's x. The spacer
                // keeps it left of the row's other buttons, which align right.
                row.child(
                    div()
                        .flex_shrink_0()
                        .child(self.render_sidebar_collapse_button(Some(appearance.muted), cx)),
                )
                // Right of Toggle sidebar, as in the collapsed header (workarea_header/breadcrumb.rs),
                // which puts it 4pt after that button: the margin tops the scaled row gap up to it.
                .child(
                    div().flex_shrink_0().ml(px(4.0 - 4.0 * scale)).child(
                        self.render_workarea_header_agents_toggle(Some(appearance.muted), cx),
                    ),
                )
                // The Board button, right of the Agents Panel one in both places.
                .when(self.session_board_enabled(), |row| {
                    row.child(
                        div().flex_shrink_0().ml(px(4.0 - 4.0 * scale)).child(
                            self.render_workarea_header_board_toggle(Some(appearance.muted), cx),
                        ),
                    )
                })
                // The negative margin cancels the row gap this zero-width spacer adds, so the
                // buttons right of it keep their even 38px slots.
                .child(div().flex_1().mr(px(-4.0 * scale)))
            })
            .when_some(bots_toggle, |row, bots_mode| {
                row.child(self.render_native_sidebar_bots_toggle(bots_mode, appearance, cx))
            })
            // CDXC:WorkMode 2026-10-09 DECISION:
            // User: a briefcase button at the top of the sidebar opens the Work list as a tab in the side panel; it shows only while the window has a project with Work mode on, Personal ones included. (Since 2026-10-24 the view is called Issues and a project has Issues Workflow on once a Linear key or `gh` is set up; CDXC:WorkMode 2026-10-24 in server/src/work_mode/project.rs.)
            .when(work_visible, |row| {
                let showing = self.work_view_showing();
                row.child(
                    div()
                        .id("native-sidebar-work")
                        .role(gpui::Role::Button)
                        .aria_label("Issues")
                        .aria_selected(showing)
                        .when(cfg!(target_os = "windows"), |button| button.occlude())
                        .h(px(28.0 * scale))
                        .w(px(34.0 * scale))
                        .rounded(px(5.0 * scale))
                        .flex()
                        .flex_shrink(1.0)
                        .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .when(showing, |button| button.bg(appearance.hover))
                        .hover(|button| button.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            "titlebar/briefcase.svg",
                            15.0 * scale,
                            if showing {
                                titlebar_active_text_color()
                            } else {
                                appearance.muted
                            },
                        ))
                        .on_click(cx.listener(|app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(json!({"type": "openWorkView"}), cx);
                        }))
                        .managed_discrete_tooltip_with_placement(
                            ManagedTooltipPlacement::Right,
                            tooltip_delay,
                            |window, cx| {
                                super::tooltips::sidebar_style_tooltip(
                                    "Issues".to_string(),
                                    window,
                                    cx,
                                )
                            },
                        ),
                )
            })
            // CDXC:Workspaces 2026-10-11 DECISION: User: move the workspace picker out of the Spaces row to the bottom left of the app, in the sidebar's bottom bar, with 5px more margin from the sidebar's left side than the bar's own padding so it lines up with the account row above it. The bar's other buttons sit right of the spacer.
            .when(footer, |row| {
                let tile = self.render_native_sidebar_footer_workspace_tile(appearance, cx);
                row.children(tile.map(|tile| div().flex_shrink_0().ml(px(5.0 * scale)).child(tile)))
                    .child(div().flex_1())
            })
            .child(
                div()
                    .id(format!("native-sidebar-{label}"))
                    .role(gpui::Role::Button)
                    .aria_label(tooltip_label.clone())
                    .when(cfg!(target_os = "windows"), |button| button.occlude())
                    .h(px(28.0 * scale))
                    .w(px(34.0 * scale))
                    .rounded(px(5.0 * scale))
                    .flex()
                    .flex_shrink(1.0)
                    .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .hover(|row| row.bg(appearance.hover))
                    .child(titlebar_svg_icon(
                        icon_path,
                        15.0 * scale,
                        titlebar_active_text_color().opacity(0.52),
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(
                            json!({"type": "sidebarAction", "action": action_id}),
                            cx,
                        );
                    }))
                    .managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::Right,
                        tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_style_tooltip(
                                tooltip_label.to_string(),
                                window,
                                cx,
                            )
                        },
                    ),
            )
            // CDXC:Notifications 2026-09-20 DECISION:
            // User: the notification bell sits in the sidebar's top row, before the sidebar menu button.
            .when(bell_visible, |row| {
                row.child(self.render_sidebar_notification_bell(appearance, cx))
            })
            .when(!footer, |row| {
                let more_open = self
                    .native_sidebar
                    .menu
                    .as_ref()
                    .is_some_and(|menu| menu.opened_by(super::menus::SIDEBAR_MORE_TRIGGER));
                row.child(
                    div()
                        .id("native-sidebar-more")
                        .role(gpui::Role::Button)
                        .aria_label("Sidebar menu")
                        .aria_expanded(more_open)
                        .when(cfg!(target_os = "windows"), |button| button.occlude())
                        .h_full()
                        .w(px(34.0 * scale))
                        .rounded(px(5.0 * scale))
                        .flex()
                        .flex_shrink(1.0)
                        .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .when(more_open, |row| row.bg(appearance.hover))
                        .hover(|row| row.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            "titlebar/menu-2.svg",
                            15.0 * scale,
                            appearance.muted,
                        ))
                        .on_prepaint({
                            let bounds = self.native_sidebar.more_button_bounds.clone();
                            move |painted, _, _| bounds.set(Some(painted))
                        })
                        .on_mouse_down(MouseButton::Left, {
                            let bounds = self.native_sidebar.more_button_bounds.clone();
                            move |event: &gpui::MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                let trigger = bounds.get().unwrap_or_else(|| gpui::Bounds {
                                    origin: event.position,
                                    size: gpui::size(gpui::px(1.0), gpui::px(1.0)),
                                });
                                Self::toggle_native_sidebar_more_menu(
                                    &more_menu_snapshot.more_menu,
                                    trigger,
                                    event.position,
                                    scale,
                                    window,
                                    cx,
                                );
                            }
                        }),
                )
            })
            /*
            CDXC:Sidebar 2026-09-21 DECISION:
            User: Settings gets a one-click gear immediately to the right of the Commands
            button. The gear is now the sidebar's only Settings entry: the sidebar menu no longer carries
            Settings or Hotkeys (packages/gx-core/src/sidebar_menu/navigation.rs). This
            supersedes the 2026-09-20 rule that kept both entries in the menu as a
            deliberate duplicate.
            */
            .when(footer, |row| {
                row.children(self.render_native_sidebar_usage_toggle(
                    appearance,
                    usage_meter_count,
                    cx,
                ))
            })
            .when(footer, |row| {
                row.child(
                    div()
                        .id("native-sidebar-settings")
                        .role(gpui::Role::Button)
                        .aria_label("Settings")
                        .h(px(28.0 * scale))
                        .w(px(34.0 * scale))
                        .rounded(px(5.0 * scale))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .hover(|row| row.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            TITLEBAR_ICON_SETTINGS,
                            15.0 * scale,
                            appearance.muted,
                        ))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(
                                json!({"type": "sidebarAction", "action": "settings"}),
                                cx,
                            );
                        }))
                        .managed_discrete_tooltip_with_placement(
                            ManagedTooltipPlacement::Right,
                            appearance.tooltip_delay,
                            |window, cx| {
                                super::tooltips::sidebar_style_tooltip(
                                    titlebar_tooltip_label("Settings", "openSettings").to_string(),
                                    window,
                                    cx,
                                )
                            },
                        ),
                )
            })
            .into_any_element()
    }
}

//! The Ghosts section at the top of the Projects list: a "Ghosts" title with + (New Ghost), then
//! one row per Ghost. A row opens the Ghost's chat; its hover buttons open the Ghost's Overview
//! panel and its settings.
//!
//! CDXC:Ghosts 2026-10-29 SEE-ALSO: packages/gx-core/src/sidebar_view/view.rs (`GhostView`, the user's decision on the section), apps/desktop/src/app/native_ghost/ (the Overview panel and the modal the buttons open), native_sidebar/two_column/nav.rs (the same section in the Two columns layout).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::consts::{TITLEBAR_ICON_LAYOUT_COLUMNS, TITLEBAR_ICON_PLUS, TITLEBAR_ICON_SETTINGS};
use crate::app::helpers::*;
use ghostex_gx_core::GhostView;

/// The colours a Ghost without a chosen one is drawn in, picked by its id.
const GHOST_COLORS: [u32; 8] = [
    0x7c9cff, 0x4fd1a5, 0xf59e0b, 0xf472b6, 0xa78bfa, 0x38bdf8, 0xfb7185, 0x94a3b8,
];

/// A Ghost's colour: its own `#rrggbb`, else one of [`GHOST_COLORS`] chosen by its id.
pub(crate) fn ghost_color(ghost_id: &str, color: &str) -> u32 {
    let own = color.trim().trim_start_matches('#');
    if own.len() == 6 {
        if let Ok(value) = u32::from_str_radix(own, 16) {
            return value;
        }
    }
    let hash = ghost_id
        .bytes()
        .fold(0u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte as u32));
    GHOST_COLORS[hash as usize % GHOST_COLORS.len()]
}

/// What a Ghost's avatar shows: its mascot (an emoji or a short word), else its initial.
pub(crate) fn ghost_glyph(name: &str, mascot: &str) -> String {
    let mascot = mascot.trim();
    if !mascot.is_empty() {
        return mascot.chars().take(2).collect();
    }
    name.trim()
        .chars()
        .next()
        .map(|initial| initial.to_uppercase().collect())
        .unwrap_or_else(|| "G".to_string())
}

/// The rounded square a Ghost is drawn as.
pub(crate) fn ghost_avatar(
    ghost_id: &str,
    name: &str,
    mascot: &str,
    color: &str,
    size: f32,
) -> AnyElement {
    let base = gpui::rgb(ghost_color(ghost_id, color));
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded(px(size * 0.28))
        .bg(gpui::Hsla::from(base).opacity(0.22))
        .border_1()
        .border_color(gpui::Hsla::from(base).opacity(0.55))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(size * 0.55))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(base)
        .child(ghost_glyph(name, mascot))
        .into_any_element()
}

impl GhostexGpuiApp {
    pub(crate) fn render_native_sidebar_ghosts_section(
        &self,
        ghosts: &[GhostView],
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let header = h_flex()
            .id("native-sidebar-ghosts-header")
            .h(px(24.0 * scale))
            .ml(px(12.0 * scale))
            .mr(px(8.0 * scale))
            .pr(px(2.0 * scale))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.5 * scale))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(appearance.muted)
                    .child("Ghosts"),
            )
            .child(
                div()
                    .id("native-sidebar-ghosts-new")
                    .role(gpui::Role::Button)
                    .aria_label("New Ghost")
                    .size(px(20.0 * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * scale))
                    .hover(|button| button.bg(appearance.hover))
                    .child(titlebar_svg_icon(TITLEBAR_ICON_PLUS, 13.0 * scale, appearance.muted))
                    .on_click(cx.listener(|app, _, window, cx| {
                        cx.stop_propagation();
                        app.open_new_ghost_modal(window, cx);
                    })),
            );
        let mut section = gpui_component::v_flex()
            .id("native-sidebar-ghosts")
            .w_full()
            .mb(px(10.0 * scale))
            .child(header);
        if ghosts.is_empty() {
            section = section.child(
                div()
                    .ml(px(12.0 * scale))
                    .mr(px(8.0 * scale))
                    .text_size(px(12.0 * scale))
                    .text_color(appearance.muted)
                    .child("A Ghost remembers, looks after your projects and runs habits. + makes one."),
            );
        }
        for ghost in ghosts {
            section = section.child(self.render_native_sidebar_ghost_row(ghost, appearance, cx));
        }
        section.into_any_element()
    }

    /// A Ghost in the Two columns layout's narrow rail: its avatar, which opens its chat.
    pub(crate) fn render_native_sidebar_ghost_rail_avatar(
        &self,
        ghost: &GhostView,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let focused = ghost.chat.as_ref().is_some_and(|chat| chat.is_focused);
        let (ghost_id, project_id, session_id) = (
            ghost.ghost_id.clone(),
            ghost.project_id.clone(),
            ghost.chat_session_id.clone(),
        );
        div()
            .id(SharedString::from(format!("native-sidebar-ghost-rail-{}", ghost.ghost_id)))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(ghost.name.clone()))
            .aria_selected(focused)
            .flex()
            .justify_center()
            .py(px(4.0 * scale))
            .rounded(px(5.0 * scale))
            .hover(|row| row.bg(appearance.session_hover))
            .when(focused, |row| row.bg(appearance.selected))
            .child(ghost_avatar(
                &ghost.ghost_id,
                &ghost.name,
                &ghost.mascot,
                &ghost.color,
                22.0 * scale,
            ))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.open_ghost(&ghost_id, &project_id, &session_id, cx);
            }))
            .into_any_element()
    }

    /// Selects a Ghost: opens its chat with its Overview beside it, and the Two columns layout drops
    /// the session column until a project is picked again (layout.rs). Its home project is never
    /// drawn as a project group, so the sidebar's own row focus cannot reach the chat; this is the
    /// open a newly created session takes.
    ///
    /// CDXC:Ghosts 2026-10-29 DECISION: when a Ghost is picked, the project the sidebar had open stays out of sight and the Overview on the right opens at once, so the Ghost is easier to control.
    pub(crate) fn open_ghost(
        &mut self,
        ghost_id: &str,
        project_id: &str,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if session_id.is_empty() {
            return;
        }
        let state = &mut self.native_sidebar.two_column;
        state.view = super::two_column::TwoColumnView::Project;
        state.selected_group = None;
        self.gx_store_focus_created_session(project_id, session_id, true, Some("chat"), cx);
        // The create open never acknowledges attention, which a row click does
        // (focus_perform.rs), so a Ghost's finished turn kept its blue dot after it was opened.
        self.gx_store_acknowledge_attention(
            ghostex_gx_core::SessionKey::local(project_id, session_id),
            cx,
        );
        self.reveal_floating_sessions(cx);
        self.show_ghost_overview_in_main_window(ghost_id, cx);
    }

    fn render_native_sidebar_ghost_row(
        &self,
        ghost: &GhostView,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let focused = ghost.chat.as_ref().is_some_and(|chat| chat.is_focused);
        let overview_showing = self.ghost_overview_showing(&ghost.ghost_id);
        let row_id = SharedString::from(format!("native-sidebar-ghost-{}", ghost.ghost_id));
        let group = SharedString::from(format!("native-sidebar-ghost-group-{}", ghost.ghost_id));
        let (project_id, session_id) = (ghost.project_id.clone(), ghost.chat_session_id.clone());
        let status = ghost.chat.as_ref().and_then(|chat| {
            if chat.row.pending_question_count > 0 {
                return Some(
                    div()
                        .size(px(super::status::STATUS_DOT_SIZE * scale))
                        .rounded_full()
                        .bg(rgb(super::status::QUESTION_COLOR))
                        .into_any_element(),
                );
            }
            super::status::activity_indicator(
                &chat.row.activity,
                chat.row.has_background_work,
                chat.row.model_selection_failed,
                scale,
            )
        });
        let row_ghost_id = ghost.ghost_id.clone();
        let overview_id = ghost.ghost_id.clone();
        let settings_id = ghost.ghost_id.clone();
        let hover_button = |id: String, icon: &'static str, label: &'static str| {
            div()
                .id(SharedString::from(id))
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(20.0 * scale))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0 * scale))
                .invisible()
                .group_hover(group.clone(), |button| button.visible())
                .hover(|button| button.bg(appearance.hover))
                .child(titlebar_svg_icon(icon, 13.0 * scale, appearance.muted))
        };
        h_flex()
            .id(row_id)
            .group(group.clone())
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(ghost.name.clone()))
            .aria_selected(focused)
            .relative()
            .h(px(30.0 * scale))
            .ml(px(6.0 * scale))
            .mr(px(8.0 * scale))
            .pl(px(6.0 * scale))
            .pr(px(4.0 * scale))
            .gap(px(8.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_default()
            .hover(|row| row.bg(appearance.session_hover))
            .when(focused, |row| {
                row.bg(appearance.selected)
                    .child(super::decorations::selected_outline(appearance))
            })
            .child(ghost_avatar(
                &ghost.ghost_id,
                &ghost.name,
                &ghost.mascot,
                &ghost.color,
                18.0 * scale,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::LIGHT)
                    .child(ghost.name.clone()),
            )
            .child(
                hover_button(
                    format!("native-sidebar-ghost-overview-{}", ghost.ghost_id),
                    TITLEBAR_ICON_LAYOUT_COLUMNS,
                    "Overview",
                )
                .when(overview_showing, |button| button.visible())
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.toggle_ghost_overview(&overview_id, window, cx);
                })),
            )
            .child(
                hover_button(
                    format!("native-sidebar-ghost-settings-{}", ghost.ghost_id),
                    TITLEBAR_ICON_SETTINGS,
                    "Ghost settings",
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.open_ghost_settings_modal(&settings_id, window, cx);
                })),
            )
            .children(status)
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.open_ghost(&row_ghost_id, &project_id, &session_id, cx);
            }))
            .into_any_element()
    }
}

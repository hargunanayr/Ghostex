//! The session column's header in the two-column layout: the selected project (its icon, name,
//! project group and session count, and its New Terminal / agent launcher / ⋯ buttons, always
//! shown), or the title of a Needs you / Running / All sessions view.

use std::{cell::Cell, rc::Rc};

use ghostex_gx_core::StatusFilter;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex, v_flex};

use super::super::appearance::SidebarAppearance;
use super::super::model::{NativeSidebarGroup, NativeSidebarSnapshot};
use super::layout::{TwoColumnFrame, project_icon};
use super::nav::hairline;
use super::state::TwoColumnView;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

const HEADER_HEIGHT: f32 = 50.0;

fn sessions_phrase(count: usize) -> String {
    format!("{count} session{}", if count == 1 { "" } else { "s" })
}

impl GhostexGpuiApp {
    pub(super) fn render_two_column_project_header(
        &self,
        group: &NativeSidebarGroup,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let id = group.group_id.clone();
        let menu = group.menu.clone();
        let header_bounds = Rc::new(Cell::new(None::<gpui::Bounds<gpui::Pixels>>));
        let menu_bounds = header_bounds.clone();
        let tint = frame.tint(content, &group.group_id);
        let count = sessions_phrase(frame.tally(&group.group_id).all);
        let subtitle = match frame.collection_of.get(&group.group_id) {
            Some(index) => format!("{} · {count}", content.collections[*index].title),
            None => count,
        };
        let title: AnyElement = if let Some(editor) = self
            .native_sidebar
            .name_editor
            .as_ref()
            .filter(|editor| editor.kind == "group" && editor.id == id)
        {
            div()
                .on_action(
                    cx.listener(|app, _: &gpui_component::input::Escape, _, cx| {
                        cx.stop_propagation();
                        app.finish_native_sidebar_rename(false, cx);
                    }),
                )
                .child(gpui_component::input::Input::new(&editor.input).h(px(24.0 * scale)))
                .into_any_element()
        } else {
            let tooltip = group.title_tooltip.clone();
            div()
                .id(format!("native-sidebar-two-column-title-{id}"))
                .min_w_0()
                .truncate()
                .text_size(px(14.5 * scale))
                .font_weight(FontWeight::MEDIUM)
                .text_color(appearance.foreground)
                .child(group.title.clone())
                .when_some(tooltip, |title, tooltip| {
                    title.managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::BelowLeft,
                        appearance.tooltip_delay,
                        move |window, cx| {
                            super::super::tooltips::sidebar_style_tooltip(
                                tooltip.clone(),
                                window,
                                cx,
                            )
                        },
                    )
                })
                .into_any_element()
        };
        h_flex()
            .id(format!("native-sidebar-two-column-header-{id}"))
            .on_prepaint(move |bounds, _, _| header_bounds.set(Some(bounds)))
            .role(gpui::Role::Group)
            .aria_label(group.title.clone())
            .h(px(HEADER_HEIGHT * scale))
            .flex_shrink_0()
            .pl(px(12.0 * scale))
            .pr(px(8.0 * scale))
            .gap(px(10.0 * scale))
            .border_b_1()
            .border_color(hairline())
            .children(
                project_icon(
                    group,
                    &content.hud,
                    tint,
                    frame.sprite(&group.group_id),
                    appearance,
                    20.0,
                )
                .map(|icon| {
                    let wash = tint
                        .unwrap_or_else(|| super::sprite::fallback_color(&group.group_id))
                        .opacity(0.16);
                    div()
                        .size(px(32.0 * scale))
                        .flex_shrink_0()
                        .rounded(px(8.0 * scale))
                        .bg(wash)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon)
                }),
            )
            .child(
                v_flex().flex_1().min_w_0().child(title).child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.5 * scale))
                        .text_color(appearance.muted)
                        .child(subtitle),
                ),
            )
            .child(
                h_flex().flex_shrink_0().gap(px(2.0 * scale)).children(
                    group
                        .header_actions
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            self.render_native_project_action(
                                &id,
                                index,
                                item,
                                menu_bounds.clone(),
                                appearance,
                                cx,
                            )
                        }),
                ),
            )
            .on_mouse_down(MouseButton::Right, {
                let menu_bounds = menu_bounds.clone();
                move |event, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_context_menu(
                        &menu,
                        menu_bounds.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                }
            })
            .into_any_element()
    }

    pub(super) fn render_two_column_smart_header(
        &self,
        filter: StatusFilter,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let count = frame.totals.count(filter);
        let projects = content
            .groups
            .iter()
            .filter(|group| frame.tally(&group.group_id).count(filter) > 0)
            .count();
        let (title, icon) = match filter {
            StatusFilter::NeedsYou => ("Needs you", "titlebar/chat-card-question.svg"),
            StatusFilter::Running => ("Running", "titlebar/loader2.svg"),
            StatusFilter::All => ("All sessions", "titlebar/stack.svg"),
        };
        let subtitle = format!(
            "{} in {projects} project{}",
            sessions_phrase(count),
            if projects == 1 { "" } else { "s" }
        );
        h_flex()
            .h(px(HEADER_HEIGHT * scale))
            .flex_shrink_0()
            .pl(px(12.0 * scale))
            .pr(px(8.0 * scale))
            .gap(px(10.0 * scale))
            .border_b_1()
            .border_color(hairline())
            .child(
                div()
                    .size(px(24.0 * scale))
                    .flex_shrink_0()
                    .rounded(px(6.0 * scale))
                    .bg(chrome_ink().opacity(0.06))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(titlebar_svg_icon(icon, 14.0 * scale, appearance.foreground)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.5 * scale))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(appearance.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.5 * scale))
                            .text_color(appearance.muted)
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .id("native-sidebar-close-view")
                    .role(gpui::Role::Button)
                    .aria_label("Back to the project")
                    .size(px(24.0 * scale))
                    .rounded(px(5.0 * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .hover(|button| button.bg(appearance.hover))
                    .child(titlebar_svg_icon(
                        "titlebar/x.svg",
                        13.0 * scale,
                        appearance.muted,
                    ))
                    .on_click(cx.listener(|app, _, _, cx| {
                        cx.stop_propagation();
                        app.native_sidebar.two_column.view = TwoColumnView::Project;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

use ghostex_gx_core::{SessionBucket, StatusFilter};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};

use super::super::appearance::SidebarAppearance;
use super::super::model::{NativeSidebarGroup, NativeSidebarSnapshot};
use super::super::status::{QUESTION_COLOR, WORKING_COLOR};
use super::buckets::{bucket_group, matching_rows};
use super::layout::{TwoColumnFrame, project_icon};
use super::nav::hairline;
use super::state::TwoColumnView;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The session column sits a shade apart from the project column, so the two read as panes.
fn session_surface(appearance: &SidebarAppearance) -> Hsla {
    if appearance.light {
        gpui::black().opacity(0.025)
    } else {
        gpui::black().opacity(0.16)
    }
}

/// A heading's rule, running from its label to its count.
fn rule() -> AnyElement {
    div().flex_1().h(px(1.0)).bg(hairline()).into_any_element()
}

impl GhostexGpuiApp {
    pub(super) fn render_two_column_sessions(
        &self,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let view = cx.entity().clone();
        let group = frame.selected_group.map(|index| &content.groups[index]);
        let (header, body) = match self.native_sidebar.two_column.view {
            TwoColumnView::Project => (
                group.map(|group| {
                    v_flex()
                        .child(self.render_two_column_project_header(
                            group, content, frame, appearance, cx,
                        ))
                        .child(self.render_two_column_filter_bar(group, frame, appearance, cx))
                        .into_any_element()
                }),
                group.map_or_else(
                    || self.render_two_column_empty("No project selected", appearance),
                    |group| self.render_two_column_project_list(group, frame, appearance, cx),
                ),
            ),
            TwoColumnView::Smart(filter) => (
                Some(self.render_two_column_smart_header(filter, content, frame, appearance, cx)),
                self.render_two_column_smart_list(filter, content, frame, appearance, cx),
            ),
        };
        v_flex()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(session_surface(appearance))
            .children(header)
            .child(
                div()
                    .on_children_prepainted(move |_, window, cx| {
                        view.update(cx, |app, cx| {
                            app.update_native_sidebar_scroll(window, cx);
                            app.update_native_space_transition(window, cx);
                        })
                    })
                    .id("native-sidebar-scroll")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.native_sidebar.scroll)
                    .on_scroll_wheel(cx.listener(|app, _, _, cx| {
                        app.native_sidebar.scroll_animation = None;
                        app.native_sidebar_scroll_wheel_moved(cx);
                    }))
                    .child(
                        v_flex()
                            .w_full()
                            .pl(px(5.0 * scale))
                            .pr(px(3.0 * scale))
                            .pb(px(24.0 * scale))
                            .child(body),
                    ),
            )
            .child(self.render_native_sidebar_list_fade(appearance))
            .into_any_element()
    }

    /// CDXC:Sidebar 2026-10-27 WHY: the shared `modal_segmented_control` takes static labels and the modal palette; this bar carries a live count in each segment and sits on the sidebar's glass, so it is drawn with the sidebar's own fills in the segmented shape (one highlighted segment on an inset track).
    fn render_two_column_filter_bar(
        &self,
        group: &NativeSidebarGroup,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let tally = frame.tally(&group.group_id);
        let current = self.native_sidebar.two_column.filter;
        h_flex()
            .id("native-sidebar-status-filter")
            .role(gpui::Role::Group)
            .aria_label("Filter sessions")
            .mx(px(10.0 * scale))
            .mt(px(10.0 * scale))
            .mb(px(2.0 * scale))
            .p(px(2.0 * scale))
            .gap(px(2.0 * scale))
            .rounded(px(8.0 * scale))
            .bg(chrome_ink().opacity(0.05))
            .border_1()
            .border_color(hairline())
            .children(
                [
                    StatusFilter::All,
                    StatusFilter::NeedsYou,
                    StatusFilter::Running,
                ]
                .into_iter()
                .map(|filter| {
                    let count = tally.count(filter);
                    let on = filter == current;
                    let count_color = match filter {
                        StatusFilter::NeedsYou if count > 0 => rgb(QUESTION_COLOR).into(),
                        StatusFilter::Running if count > 0 => rgb(WORKING_COLOR).into(),
                        _ => appearance.muted,
                    };
                    div()
                        .id(format!("native-sidebar-status-filter-{}", filter.label()))
                        .role(gpui::Role::Button)
                        .aria_label(format!("{}, {count}", filter.label()))
                        .aria_selected(on)
                        .flex_1()
                        .min_w_0()
                        .h(px(26.0 * scale))
                        .rounded(px(6.0 * scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(5.0 * scale))
                        .cursor_default()
                        .text_size(px(12.5 * scale))
                        .text_color(if on {
                            appearance.foreground
                        } else {
                            appearance.muted
                        })
                        .when(on, |segment| segment.bg(appearance.selected))
                        .when(!on, |segment| {
                            segment.hover(|segment| segment.bg(appearance.hover))
                        })
                        .when(count == 0 && filter != StatusFilter::All, |segment| {
                            segment.opacity(0.55)
                        })
                        .child(div().truncate().child(filter.label()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0 * scale))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(count_color)
                                .child(count.to_string()),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.native_sidebar.two_column.filter = filter;
                            cx.notify();
                        }))
                }),
            )
            .into_any_element()
    }

    fn render_two_column_project_list(
        &self,
        group: &NativeSidebarGroup,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let filter = self.native_sidebar.two_column.filter;
        let buckets = bucket_group(group, filter, frame.parking, frame.day);
        if buckets.is_empty() {
            return self.render_two_column_empty(
                match filter {
                    StatusFilter::All => "No sessions yet",
                    StatusFilter::NeedsYou => "Nothing here needs you",
                    StatusFilter::Running => "Nothing running here",
                },
                appearance,
            );
        }
        v_flex()
            .w_full()
            .children(buckets.into_iter().map(|bucketed| {
                let collapsed = self
                    .native_sidebar
                    .two_column
                    .bucket_collapsed(&group.group_id, bucketed.bucket);
                v_flex()
                    .w_full()
                    .child(self.render_two_column_bucket_heading(
                        &group.group_id,
                        bucketed.bucket,
                        bucketed.rows.len(),
                        collapsed,
                        appearance,
                        cx,
                    ))
                    .when(!collapsed, |block| {
                        block.child(self.render_native_session_list(
                            group,
                            bucketed.rows,
                            appearance,
                            cx,
                        ))
                    })
            }))
            .into_any_element()
    }

    fn render_two_column_bucket_heading(
        &self,
        group_id: &str,
        bucket: SessionBucket,
        count: usize,
        collapsed: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let toggle_id = group_id.to_owned();
        let accent = match bucket {
            SessionBucket::NeedsYou => Some(rgb(QUESTION_COLOR)),
            SessionBucket::Running => Some(rgb(WORKING_COLOR)),
            _ => None,
        };
        h_flex()
            .id(format!(
                "native-sidebar-bucket-{group_id}-{}",
                bucket.as_str()
            ))
            .role(gpui::Role::Button)
            .aria_label(format!("{} heading, {count}", bucket.label()))
            .aria_expanded(!collapsed)
            .mx(px(3.0 * scale))
            .mt(px(12.0 * scale))
            .mb(px(3.0 * scale))
            .h(px(20.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(7.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_default()
            .text_size(px(11.0 * scale))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(appearance.muted.opacity(0.85))
            .hover(|row| row.bg(chrome_ink().opacity(0.05)))
            .children(accent.map(|color| {
                div()
                    .size(px(6.0 * scale))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(color)
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .when_some(accent, |label, color| label.text_color(color))
                    .child(bucket.label().to_uppercase()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::NORMAL)
                    .child(count.to_string()),
            )
            .child(rule())
            .child(
                gpui::svg()
                    .path(if collapsed {
                        crate::app::consts::COMMAND_ICON_CHEVRON_RIGHT
                    } else {
                        crate::app::consts::COMMAND_ICON_CHEVRON_DOWN
                    })
                    .size(px(11.0 * scale))
                    .flex_shrink_0()
                    .text_color(appearance.muted.opacity(0.7)),
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.native_sidebar
                    .two_column
                    .toggle_bucket(&toggle_id, bucket);
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_two_column_smart_list(
        &self,
        filter: StatusFilter,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let blocks: Vec<AnyElement> = frame
            .projects()
            .map(|(group, _)| &content.groups[group])
            .filter_map(|group| {
                let rows = matching_rows(group, filter, frame.parking, frame.day);
                if rows.is_empty() {
                    return None;
                }
                let id = group.group_id.clone();
                let tint = frame.tint(content, &group.group_id);
                Some(
                    v_flex()
                        .w_full()
                        .child(
                            h_flex()
                                .id(format!("native-sidebar-smart-project-{id}"))
                                .role(gpui::Role::Button)
                                .aria_label(format!("Open {}", group.title))
                                .mx(px(3.0 * scale))
                                .mt(px(12.0 * scale))
                                .mb(px(3.0 * scale))
                                .h(px(24.0 * scale))
                                .px(px(6.0 * scale))
                                .gap(px(8.0 * scale))
                                .rounded(px(5.0 * scale))
                                .cursor_default()
                                .text_size(px(12.5 * scale))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(appearance.foreground.opacity(0.8))
                                .hover(|row| row.bg(appearance.hover))
                                .children(project_icon(
                                    group,
                                    &content.hud,
                                    tint,
                                    frame.sprite(&group.group_id),
                                    appearance,
                                    15.0,
                                ))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .max_w(px(160.0 * scale))
                                        .truncate()
                                        .child(group.title.clone()),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(appearance.muted)
                                        .font_weight(FontWeight::NORMAL)
                                        .child(rows.len().to_string()),
                                )
                                .child(rule())
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.native_sidebar.two_column.filter = filter;
                                    app.open_two_column_project(&id, cx);
                                })),
                        )
                        .child(self.render_native_session_list(group, rows, appearance, cx))
                        .into_any_element(),
                )
            })
            .collect();
        if blocks.is_empty() {
            return self.render_two_column_empty(
                match filter {
                    StatusFilter::All => "No sessions yet",
                    StatusFilter::NeedsYou => "Nothing is waiting on you",
                    StatusFilter::Running => "Nothing is running",
                },
                appearance,
            );
        }
        v_flex().w_full().children(blocks).into_any_element()
    }

    fn render_two_column_empty(
        &self,
        text: &'static str,
        appearance: &SidebarAppearance,
    ) -> AnyElement {
        let scale = appearance.scale;
        div()
            .w_full()
            .pt(px(32.0 * scale))
            .flex()
            .justify_center()
            .text_size(px(13.0 * scale))
            .text_color(appearance.muted)
            .child(text)
            .into_any_element()
    }
}

//! The project column of the two-column layout: the Needs you / Running / All sessions views, the
//! pinned orchestrators, the projects in their project-group cards, and the Spaces row at its foot.
//!
//! CDXC:Sidebar 2026-10-27 DECISION:
//! User, looking at the first build: make it look like the approved mockup (docs/2026-10-27/sidebar-alternatives/alt-a-two-column.html), "when I click a project on the left I want it to already open a session so it switches straight to that project", and "move the Space picker to the very bottom of the project sidebar". A project row opens the project the way Quick Access does (its last session, or its default agent when it has none, `gx_store_activate_project`); a user-made session group opens its most recent session. The Spaces row and the machine tabs sit at the foot of this column, centred in it, and stack in a column while it is an icon rail; Classic keeps them under the top row.

use std::sync::Arc;

use ghostex_gx_core::StatusFilter;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::{Value, json};

use super::super::appearance::SidebarAppearance;
use super::super::model::{NativeSidebarGroup, NativeSidebarSession, NativeSidebarSnapshot};
use super::super::selectors::SelectorsPlacement;
use super::super::status::{QUESTION_COLOR, WORKING_COLOR};
use super::buckets::Tally;
use super::layout::{NavBlock, TwoColumnFrame, collection_color, project_icon};
use super::state::TwoColumnView;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The light blue of a finished turn the user has not seen (status.rs `activity_indicator`).
pub(super) const UNSEEN_COLOR: u32 = 0x95d7f6;

/// The hairline both columns draw their edges and rules with.
pub(super) fn hairline() -> gpui::Rgba {
    chrome_ink().opacity(0.07)
}

/// The count a Needs you badge shows, pink while an answer is waited on.
pub(super) fn needs_badge(tally: Tally, scale: f32) -> Option<AnyElement> {
    (tally.needs_you > 0).then(|| {
        div()
            .min_w(px(16.0 * scale))
            .h(px(16.0 * scale))
            .px(px(5.0 * scale))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .bg(rgb(if tally.waits_on_answer > 0 {
                QUESTION_COLOR
            } else {
                UNSEEN_COLOR
            }))
            .text_color(rgb(0x111111))
            .text_size(px(10.5 * scale))
            .font_weight(FontWeight::SEMIBOLD)
            .child(tally.needs_you.to_string())
            .into_any_element()
    })
}

fn running_dot(scale: f32) -> AnyElement {
    div()
        .size(px(7.0 * scale))
        .rounded_full()
        .flex_shrink_0()
        .bg(rgb(WORKING_COLOR))
        .into_any_element()
}

/// A project's state at the end of its row: the Needs you count, else a working dot.
fn project_state(tally: Tally, scale: f32) -> Option<AnyElement> {
    needs_badge(tally, scale).or_else(|| (tally.running > 0).then(|| running_dot(scale)))
}

/// The project's uncommitted line counts, in the project header's colours.
fn diff_stats(group: &NativeSidebarGroup, hud: &Value, scale: f32) -> Option<AnyElement> {
    if hud["settings"]["hideProjectHeaderDiffStats"] == true {
        return None;
    }
    let stats = group
        .project_context
        .as_ref()?
        .get("editor")?
        .get("diffStats")?;
    let additions = stats["additions"].as_u64().unwrap_or_default().min(9999);
    let deletions = stats["deletions"].as_u64().unwrap_or_default().min(9999);
    if additions == 0 && deletions == 0 {
        return None;
    }
    // CDXC:Theming 2026-09-22 DECISION (project_status.rs): the numbers at 70% opacity.
    Some(
        h_flex()
            .flex_shrink_0()
            .gap(px(4.0 * scale))
            .text_size(px(11.5 * scale))
            .opacity(0.7)
            .child(
                div()
                    .text_color(chrome_color(0x9cc894, 0x2a7a3a))
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .text_color(chrome_color(0xdca8a4, 0xc43a42))
                    .child(format!("-{deletions}")),
            )
            .into_any_element(),
    )
}

fn section_label(text: &'static str, appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .pt(px(14.0 * scale))
        .pb(px(5.0 * scale))
        .px(px(9.0 * scale))
        .text_size(px(10.5 * scale))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(appearance.muted.opacity(0.8))
        .child(text.to_uppercase())
        .into_any_element()
}

impl GhostexGpuiApp {
    pub(super) fn render_two_column_nav(
        &self,
        content: &NativeSidebarSnapshot,
        selectors: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let view = self.native_sidebar.two_column.view;
        let selected_id = frame
            .selected_group
            .map(|index| content.groups[index].group_id.clone());
        let smart_views = [
            (StatusFilter::NeedsYou, "titlebar/chat-card-question.svg"),
            (StatusFilter::Running, "titlebar/loader2.svg"),
            (StatusFilter::All, "titlebar/stack.svg"),
        ];
        let pad = if frame.rail { 5.0 } else { 7.0 };
        v_flex()
            .id("native-sidebar-two-column-nav")
            .role(gpui::Role::Navigation)
            .aria_label("Projects and views")
            .when(frame.ghost_selected, |nav| nav.flex_1().min_w_0())
            .when(!frame.ghost_selected, |nav| {
                nav.w(px(frame.nav_width))
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(hairline())
            })
            .h_full()
            .child(
                v_flex()
                    .px(px(pad * scale))
                    .pt(px(2.0 * scale))
                    .pb(px(4.0 * scale))
                    .gap(px(1.0 * scale))
                    .children(smart_views.into_iter().map(|(filter, icon)| {
                        self.render_two_column_smart_view(
                            filter,
                            icon,
                            frame,
                            view == TwoColumnView::Smart(filter),
                            appearance,
                            cx,
                        )
                    })),
            )
            .child(
                div()
                    .id("native-sidebar-two-column-nav-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.native_sidebar.two_column.nav_scroll)
                    .child(
                        v_flex()
                            .px(px(pad * scale))
                            .pb(px(16.0 * scale))
                            .when(!content.bots_mode && !frame.rail, |column| {
                                column.child(div().pt(px(10.0 * scale)).child(
                                    self.render_native_sidebar_ghosts_section(
                                        &content.ghosts,
                                        appearance,
                                        cx,
                                    ),
                                ))
                            })
                            .when(
                                !content.bots_mode && frame.rail && !content.ghosts.is_empty(),
                                |column| {
                                    column
                                        .child(rail_rule(None, appearance))
                                        .children(content.ghosts.iter().map(|ghost| {
                                            self.render_native_sidebar_ghost_rail_avatar(
                                                ghost, appearance, cx,
                                            )
                                        }))
                                },
                            )
                            .when(!frame.pinned_agents.is_empty(), |column| {
                                column
                                    .when(!frame.rail, |column| {
                                        column.child(section_label("Pinned agents", appearance))
                                    })
                                    .when(frame.rail, |column| {
                                        column.child(rail_rule(None, appearance))
                                    })
                                    .children(frame.pinned_agents.iter().map(|(_, session)| {
                                        self.render_two_column_pinned_agent(
                                            session, frame, appearance, cx,
                                        )
                                    }))
                            })
                            .when(!frame.rail, |column| {
                                column.child(section_label("Projects", appearance))
                            })
                            .children(frame.blocks.iter().map(|block| {
                                self.render_two_column_block(
                                    block,
                                    content,
                                    frame,
                                    view,
                                    selected_id.as_deref(),
                                    appearance,
                                    cx,
                                )
                            })),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(hairline())
                    .child(self.render_native_sidebar_selectors_at(
                        selectors,
                        SelectorsPlacement::ProjectColumn {
                            width: frame.nav_width,
                            vertical: frame.rail,
                        },
                        appearance,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_two_column_smart_view(
        &self,
        filter: StatusFilter,
        icon: &'static str,
        frame: &TwoColumnFrame,
        selected: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = if filter == StatusFilter::All {
            "All sessions"
        } else {
            filter.label()
        };
        let count = frame.totals.count(filter);
        let trailing = match filter {
            StatusFilter::NeedsYou => needs_badge(frame.totals, scale),
            StatusFilter::Running => (count > 0).then(|| {
                div()
                    .text_size(px(12.0 * scale))
                    .text_color(rgb(WORKING_COLOR))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(count.to_string())
                    .into_any_element()
            }),
            StatusFilter::All => Some(
                div()
                    .text_size(px(12.0 * scale))
                    .text_color(appearance.muted)
                    .child(count.to_string())
                    .into_any_element(),
            ),
        };
        let row = div()
            .id(format!("native-sidebar-view-{}", label.replace(' ', "-")))
            .role(gpui::Role::Button)
            .aria_label(format!("{label}, {count}"))
            .aria_selected(selected)
            .relative()
            .h(px(28.0 * scale))
            .rounded(px(6.0 * scale))
            .flex()
            .items_center()
            .gap(px(9.0 * scale))
            .cursor_default()
            .text_color(if selected {
                appearance.foreground
            } else {
                appearance.foreground.opacity(0.82)
            })
            .when(selected, |row| row.bg(appearance.selected))
            .hover(|row| row.bg(appearance.hover))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.native_sidebar.two_column.view = TwoColumnView::Smart(filter);
                cx.notify();
            }));
        let icon = titlebar_svg_icon(icon, 14.0 * scale, appearance.muted);
        if frame.rail {
            return row
                .justify_center()
                .child(icon)
                .when_some(
                    trailing.filter(|_| filter != StatusFilter::All),
                    |row, badge| {
                        row.child(
                            div()
                                .absolute()
                                .top(px(-2.0 * scale))
                                .right(px(-1.0 * scale))
                                .child(badge),
                        )
                    },
                )
                .managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::super::tooltips::sidebar_style_tooltip(
                            format!("{label} ({count})"),
                            window,
                            cx,
                        )
                    },
                )
                .into_any_element();
        }
        row.px(px(9.0 * scale))
            .child(icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.5 * scale))
                    .child(label),
            )
            .children(trailing)
            .into_any_element()
    }

    /// A pinned orchestrator: its crown, its name, and its crew badge or status dot. A click opens
    /// it like its session card.
    fn render_two_column_pinned_agent(
        &self,
        session: &Arc<NativeSidebarSession>,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let id = session.session_id.clone();
        let title = session.title().to_owned();
        let (focused, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            false,
            session.is_focused,
            session.is_visible,
        );
        let trailing =
            super::super::threads::coordinator_badge(session, appearance).or_else(|| {
                super::super::status::activity_indicator(
                    &session.activity,
                    session.has_background_work,
                    session.model_selection_failed || session.program_status_failed,
                    scale,
                )
            });
        let row = div()
            .id(format!("native-sidebar-pinned-agent-{id}"))
            .role(gpui::Role::Button)
            .aria_label(format!("{title}, pinned agent"))
            .aria_selected(focused)
            .relative()
            .h(px(28.0 * scale))
            .rounded(px(6.0 * scale))
            .flex()
            .items_center()
            .gap(px(8.0 * scale))
            .cursor_default()
            .when(focused, |row| row.bg(appearance.selected))
            .hover(|row| row.bg(appearance.hover))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "selectSession", "sessionId": id, "mode": "focus"}),
                    cx,
                );
                let _ = app.react_to_native_sidebar_session_click(&id, cx);
                app.reveal_floating_sessions(cx);
            }));
        let crown = super::super::threads::coordinator_icon(appearance);
        if frame.rail {
            return row
                .justify_center()
                .child(crown)
                .managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::super::tooltips::sidebar_style_tooltip(title.clone(), window, cx)
                    },
                )
                .into_any_element();
        }
        row.pl(px(9.0 * scale))
            .pr(px(2.0 * scale))
            .child(crown)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0 * scale))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .children(trailing)
            .into_any_element()
    }

    /// A project group's card, or the card of the projects outside any group.
    #[allow(clippy::too_many_arguments)]
    fn render_two_column_block(
        &self,
        block: &NavBlock,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        view: TwoColumnView,
        selected_id: Option<&str>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let collection = block.collection.map(|index| &content.collections[index]);
        let color = collection.and_then(|collection| collection_color(&collection.color));
        let collapsed = collection.is_some_and(|collection| collection.collapsed);
        let rows: Vec<AnyElement> = if collapsed {
            Vec::new()
        } else {
            block
                .projects
                .iter()
                .map(|(group, depth)| {
                    let group = &content.groups[*group];
                    let selected = view == TwoColumnView::Project
                        && selected_id == Some(group.group_id.as_str());
                    self.render_two_column_project(
                        group, *depth, selected, color, content, frame, appearance, cx,
                    )
                })
                .collect()
        };
        if frame.rail {
            return v_flex()
                .gap(px(1.0 * scale))
                .child(rail_rule(color, appearance))
                .children(rows)
                .into_any_element();
        }
        let mut tally = Tally::default();
        for (group, _) in &block.projects {
            tally.add(frame.tally(&content.groups[*group].group_id));
        }
        let header = match collection {
            Some(collection) => {
                let id = collection.collection_id.clone();
                let menu = collection.menu.clone();
                let owner = super::super::menus::MenuOwner::default();
                let menu_owner = owner.clone();
                h_flex()
                    .id(format!("native-sidebar-two-column-collection-{id}"))
                    .on_prepaint(super::super::menus::record_menu_owner(&owner))
                    .gap(px(5.0 * scale))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{} project group", collection.title))
                    .aria_expanded(!collapsed)
                    .rounded(px(6.0 * scale))
                    .cursor_default()
                    .hover(|row| row.bg(appearance.hover))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(
                            json!({ "type": "collectionAction", "collectionId": id, "action": "toggle" }),
                            cx,
                        );
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            Self::show_native_sidebar_context_menu(
                                &menu,
                                menu_owner.get(),
                                event.position,
                                scale,
                                window,
                                cx,
                            );
                        }),
                    )
                    .child(
                        gpui::svg()
                            .path(crate::app::consts::COMMAND_ICON_CHEVRON_RIGHT)
                            .size(px(12.0 * scale))
                            .flex_shrink_0()
                            .text_color(appearance.muted)
                            .with_transformation(gpui::Transformation::rotate(gpui::percentage(
                                if collapsed { 0.0 } else { 0.25 },
                            ))),
                    )
                    .child(block_title(&collection.title, color, appearance))
                    .into_any_element()
            }
            None => h_flex()
                .gap(px(5.0 * scale))
                .child(div().w(px(12.0 * scale)).flex_shrink_0())
                .child(block_title(
                    if content.collections.is_empty() {
                        "Projects"
                    } else {
                        "Ungrouped"
                    },
                    None,
                    appearance,
                ))
                .into_any_element(),
        };
        v_flex()
            .mb(px(6.0 * scale))
            .p(px(3.0 * scale))
            .rounded(px(9.0 * scale))
            .bg(chrome_ink().opacity(0.03))
            .border_1()
            .border_color(hairline())
            .child(
                h_flex()
                    .h(px(26.0 * scale))
                    .pl(px(4.0 * scale))
                    .pr(px(5.0 * scale))
                    .gap(px(6.0 * scale))
                    .child(div().flex_1().min_w_0().child(header))
                    .child(needs_badge(tally, scale).unwrap_or_else(|| {
                        div()
                            .text_size(px(11.0 * scale))
                            .text_color(appearance.muted.opacity(0.8))
                            .child(block.projects.len().to_string())
                            .into_any_element()
                    })),
            )
            .children(rows)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_two_column_project(
        &self,
        group: &NativeSidebarGroup,
        depth: u8,
        selected: bool,
        tint: Option<Hsla>,
        content: &NativeSidebarSnapshot,
        frame: &TwoColumnFrame,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let id = group.group_id.clone();
        let menu = group.menu.clone();
        let owner = super::super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let tally = frame.tally(&group.group_id);
        let title = group.title.clone();
        let row = div()
            .id(format!("native-sidebar-two-column-project-{id}"))
            .on_prepaint(super::super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(format!("Open {title}"))
            .aria_selected(selected)
            .relative()
            .h(px(28.0 * scale))
            .rounded(px(6.0 * scale))
            .flex()
            .items_center()
            .gap(px(8.0 * scale))
            .cursor_default()
            .when(group.is_stale, |row| row.opacity(0.55))
            .when(selected, |row| row.bg(appearance.selected))
            .hover(|row| row.bg(appearance.hover))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.open_two_column_project(&id, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_context_menu(
                        &menu,
                        menu_owner.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                }),
            );
        let icon = project_icon(
            group,
            &content.hud,
            tint,
            frame.sprite(&group.group_id),
            appearance,
            18.0,
        );
        if frame.rail {
            return row
                .justify_center()
                .child(icon.unwrap_or_else(|| {
                    div()
                        .text_size(px(11.0 * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.chars().next().map(String::from).unwrap_or_default())
                        .into_any_element()
                }))
                .when_some(project_state(tally, scale), |row, state| {
                    row.child(
                        div()
                            .absolute()
                            .top(px(-1.0 * scale))
                            .right(px(1.0 * scale))
                            .child(state),
                    )
                })
                .managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::super::tooltips::sidebar_style_tooltip(title.clone(), window, cx)
                    },
                )
                .into_any_element();
        }
        row.pl(px((6.0 + f32::from(depth) * 12.0) * scale))
            .pr(px(6.0 * scale))
            .children(icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.5 * scale))
                    .text_color(if selected {
                        appearance.foreground
                    } else {
                        appearance.foreground.opacity(0.85)
                    })
                    .child(group.title.clone()),
            )
            .children(
                project_state(tally, scale).or_else(|| diff_stats(group, &content.hud, scale)),
            )
            .into_any_element()
    }

    /// Opens a project from the project column: its last session, or its default agent when it
    /// has none, like Quick Access; a user-made session group opens its most recent session.
    pub(super) fn open_two_column_project(&mut self, group_id: &str, cx: &mut gpui::Context<Self>) {
        let state = &mut self.native_sidebar.two_column;
        state.view = TwoColumnView::Project;
        state.selected_group = Some(group_id.to_owned());
        cx.notify();
        let Some(snapshot) = self.native_sidebar.snapshot.clone() else {
            return;
        };
        let Some(group) = snapshot
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
        else {
            return;
        };
        if group.owner_group_id.is_some() {
            if let Some(session) = group
                .sessions
                .iter()
                .filter(|session| !session.is_browser())
                .max_by(|a, b| a.last_interaction_at.cmp(&b.last_interaction_at))
            {
                let id = session.session_id.clone();
                self.dispatch_native_sidebar_ui(
                    json!({"type": "selectSession", "sessionId": id, "mode": "focus"}),
                    cx,
                );
                let _ = self.react_to_native_sidebar_session_click(&id, cx);
            }
            return;
        }
        if let Some(project) = ghostex_gx_core::ProjectKey::parse_sidebar_group_id(group_id) {
            self.gx_store_activate_project(&project.to_workspace_project_id(), cx);
        }
    }
}

/// A project group's name in its card header, with its colour dot.
fn block_title(title: &str, color: Option<Hsla>, appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    h_flex()
        .min_w_0()
        .gap(px(7.0 * scale))
        .child(
            div()
                .size(px(7.0 * scale))
                .rounded_full()
                .flex_shrink_0()
                .bg(color.unwrap_or(appearance.muted.opacity(0.6))),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(11.5 * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(appearance.foreground.opacity(0.78))
                .child(title.to_uppercase()),
        )
        .into_any_element()
}

/// In the icon rail, a short rule in a project group's colour stands for its card.
fn rail_rule(color: Option<Hsla>, appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .h(px(14.0 * scale))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(18.0 * scale))
                .h(px(2.0 * scale))
                .rounded_full()
                .bg(color.unwrap_or(appearance.muted.opacity(0.35))),
        )
        .into_any_element()
}

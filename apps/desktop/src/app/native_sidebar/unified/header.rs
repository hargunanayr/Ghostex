//! The Unified sidebar's header: the search field, the filter, Add project and New session.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::input::Input;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::{Value, json};

use super::model::UnifiedSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// The filter menu: projects (by repository), machines and Spaces, each a pick that narrows the
/// list, ticked while it does.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q788): the header's filter narrows by projects, machines and Spaces in one menu, which stands in for the machine tabs and Space tiles the Unified sidebar does not draw.
pub(super) fn filter_menu_items(unified: &UnifiedSidebarSnapshot) -> Value {
    let filter = &unified.filter;
    let mut items = vec![json!({
        "label": "All sessions",
        "checked": filter.is_empty(),
        "command": {"type": "unifiedFilter", "kind": "clear"},
    })];
    let mut section = |title: &str,
                       kind: &str,
                       options: &[ghostex_gx_core::unified_sidebar::UnifiedFilterOption],
                       picked: &std::collections::BTreeSet<String>| {
        if options.len() < 2 && picked.is_empty() {
            return;
        }
        items.push(json!({"separator": true}));
        items.push(json!({"label": title, "heading": true}));
        for option in options {
            let mut item = json!({
                "label": option.label,
                "checked": picked.contains(&option.key),
                "command": {"type": "unifiedFilter", "kind": kind, "key": option.key},
            });
            if let Some(detail) = &option.detail {
                item["suffix"] = Value::String(detail.clone());
            }
            items.push(item);
        }
    };
    // The two short lists first, so they are not lost below a long list of projects.
    section(
        "Machines",
        "machine",
        &unified.options.machines,
        &filter.machines,
    );
    section("Spaces", "space", &unified.options.spaces, &filter.spaces);
    section("Projects", "repo", &unified.options.repos, &filter.repos);
    Value::Array(items)
}

/// What the filter button says: everything, or the first pick and how many more.
fn filter_label(unified: &UnifiedSidebarSnapshot) -> String {
    let filter = &unified.filter;
    let picked: Vec<&str> = unified
        .options
        .repos
        .iter()
        .filter(|option| filter.repos.contains(&option.key))
        .chain(
            unified
                .options
                .machines
                .iter()
                .filter(|option| filter.machines.contains(&option.key)),
        )
        .chain(
            unified
                .options
                .spaces
                .iter()
                .filter(|option| filter.spaces.contains(&option.key)),
        )
        .map(|option| option.label.as_str())
        .collect();
    match picked.as_slice() {
        [] => "All sessions".to_string(),
        [one] => (*one).to_string(),
        [first, rest @ ..] => format!("{first} +{}", rest.len()),
    }
}

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-10-30 DECISION:
    /// User: "please make the sidebar filter in unified sidebar mode not take all the height like it does now". The filter menu is a dropdown: hung under the filter button with its left edge on the button's, no taller than the app's other dropdown menus (`TITLEBAR_POPUP_MENU_MAX_HEIGHT`) or the room left below it in the window, shrinking to fit a short list, and its rows scroll inside it with the bar showing only while they scroll. Pressing the button again closes it.
    fn show_unified_filter_menu(
        menu: &Value,
        owner: Option<gpui::Bounds<gpui::Pixels>>,
        event: &gpui::ClickEvent,
        scale: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        use crate::app::native_sidebar::menus::{SidebarMenuOpener, press_position};
        let Some(owner) = owner else {
            Self::show_native_sidebar_menu(menu, event.position(), scale, window, cx);
            return;
        };
        Self::show_native_sidebar_menu_from(
            menu,
            SidebarMenuOpener::below(owner)
                .toggled_by("native-unified-filter", press_position(event))
                .capped(crate::app::consts::TITLEBAR_POPUP_MENU_MAX_HEIGHT),
            scale,
            window,
            cx,
        );
    }

    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (Q854, "field in header"): the header's first control is a search field that is always there and types into the list; the filter shrinks to an icon beside it, marked while a pick is on, and its menu still closes after each pick. Escape clears the field.
    pub(in crate::app::native_sidebar) fn render_native_unified_header(
        &mut self,
        unified: &UnifiedSidebarSnapshot,
        appearance: &SidebarAppearance,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let input = self.unified_search_input(window, cx);
        let icon_button = |id: &'static str, label: String, icon: &'static str| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(26.0 * scale))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0 * scale))
                .hover(|button| button.bg(appearance.hover))
                .child(
                    gpui::svg()
                        .path(icon)
                        .size(px(15.0 * scale))
                        .text_color(appearance.chrome_icon),
                )
        };
        let filtered = !unified.filter.is_empty();
        let menu = filter_menu_items(unified);
        let owner = crate::app::native_sidebar::menus::MenuOwner::default();
        let searching = !unified.filter.query.is_empty();
        let clear_input = input.clone();
        let escape_input = input.clone();
        let tooltip_label = filter_label(unified);
        let tooltip_delay = appearance.tooltip_delay;
        h_flex()
            .w_full()
            .flex_shrink_0()
            .h(px(36.0 * scale))
            .px(px(8.0 * scale))
            .gap(px(2.0 * scale))
            .child(
                h_flex()
                    .id("native-unified-search")
                    .min_w_0()
                    .flex_1()
                    .h(px(28.0 * scale))
                    .pl(px(8.0 * scale))
                    .pr(px(4.0 * scale))
                    .gap(px(6.0 * scale))
                    .rounded(px(6.0 * scale))
                    .border_1()
                    .border_color(appearance.foreground.opacity(0.1))
                    .on_key_down(
                        cx.listener(move |_, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                escape_input
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .child(
                        gpui::svg()
                            .path("titlebar/search.svg")
                            .size(px(14.0 * scale))
                            .flex_shrink_0()
                            .text_color(appearance.chrome_icon),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .p_0()
                                .text_size(px(13.0 * scale))
                                .text_color(appearance.foreground)
                                .placeholder_color(appearance.muted),
                        ),
                    )
                    .when(searching, |field| {
                        field.child(
                            div()
                                .id("native-unified-search-clear")
                                .role(gpui::Role::Button)
                                .aria_label("Clear search")
                                .size(px(18.0 * scale))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0 * scale))
                                .hover(|button| button.bg(appearance.hover))
                                .child(
                                    gpui::svg()
                                        .path("titlebar/x.svg")
                                        .size(px(12.0 * scale))
                                        .text_color(appearance.muted),
                                )
                                .on_click(cx.listener(move |_, _, window, cx| {
                                    cx.stop_propagation();
                                    clear_input
                                        .update(cx, |input, cx| input.set_value("", window, cx));
                                })),
                        )
                    }),
            )
            .child(
                icon_button(
                    "native-unified-filter",
                    format!("Filter sessions: {tooltip_label}"),
                    "titlebar/filter.svg",
                )
                .on_prepaint(crate::app::native_sidebar::menus::record_menu_owner(&owner))
                .when(filtered, |button| button.bg(appearance.selected))
                .when(self.native_sidebar.pointer_inside, |button| {
                    button.managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::BelowLeft,
                        tooltip_delay,
                        move |window, cx| {
                            crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                                tooltip_label.clone(),
                                window,
                                cx,
                            )
                        },
                    )
                })
                .on_click(cx.listener(
                    move |_, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        Self::show_unified_filter_menu(
                            &menu,
                            owner.get(),
                            event,
                            scale,
                            window,
                            cx,
                        );
                    },
                )),
            )
            .child(
                icon_button(
                    "native-unified-add-project",
                    "Add project".to_string(),
                    "titlebar/folder-plus.svg",
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_native_sidebar_ui(
                        json!({"type": "sidebarAction", "action": "addProject"}),
                        cx,
                    );
                })),
            )
            .child(
                icon_button(
                    "native-unified-new-session",
                    "New session".to_string(),
                    "titlebar/edit.svg",
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.open_unified_new_session(cx);
                })),
            )
            .into_any_element()
    }
}

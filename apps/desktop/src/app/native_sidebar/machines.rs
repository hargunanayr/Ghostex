use super::{appearance::SidebarAppearance, model::NativeSidebarSnapshot};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::json;

impl GhostexGpuiApp {
    pub(crate) fn render_native_machine_tabs(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if snapshot.machines.len() <= 1 {
            return None;
        }
        let scale = appearance.scale;
        Some(h_flex().w_full().h(px(28.0 * scale)).border_y_1().border_color(appearance.foreground.opacity(0.12))
            .children(snapshot.machines.iter().enumerate().map(|(index, machine)| {
                let id = machine.id.clone();
                let reconnect = id.clone();
                let menu_id = id.clone();
                let owner = super::menus::MenuOwner::default();
                let menu_owner = owner.clone();
                let workspace_menu = (id != "local").then(|| self.gx_store_machine_workspace_menu(&id)).flatten();
                let selected = snapshot.selected_machine_id == id;
                let dot_group = format!("native-machine-dots-{id}");
                let busy = ghostex_gx_core::machine_state_is_busy(&machine.state);
                let failed = id != "local" && ghostex_gx_core::machine_state_is_failure(&machine.state);
                // CDXC:RemoteMachines 2026-10-16 DECISION: the machine's Ghostex update shows on its tab (an arrow, a spinner while it installs) and as the tab menu's first item (remote_conn/app_update.rs).
                let app_update = (id != "local").then(|| self.remote_app_update_tab_badge(&id)).flatten();
                // CDXC:Sidebar 2026-10-29 DECISION: User: "remove this tooltip in the sidebar" (hovering a machine tab showed only its own label). The tab shows a tooltip only when it says more than the label: the machine's error, or a waiting Ghostex update.
                let tooltip = (machine.message.is_some() || app_update.is_some()).then(|| {
                    let tooltip = machine.message.as_ref().map(|message| format!("{}: {message}", machine.label)).unwrap_or_else(|| machine.label.clone());
                    match app_update.as_ref() { Some(update) => format!("{tooltip} · {}", update.tooltip), None => tooltip }
                });
                let update_menu_label = app_update.as_ref().and_then(|update| update.menu_label.clone());
                let update_glyph = app_update.as_ref().map(|update| {
                    let icon = gpui::svg().path(if update.busy { "titlebar/loader2.svg" } else { "titlebar/download.svg" }).size(px(12.0 * scale)).flex_shrink_0().text_color(appearance.foreground);
                    if update.busy { icon.with_throttled_animation(format!("native-machine-update-{id}"), std::time::Duration::from_millis(900), |icon, progress| icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(progress)))).into_any_element() } else { icon.into_any_element() }
                });
                let icon = gpui::svg().path(if id == "local" { "titlebar/device-desktop.svg" } else if busy { "titlebar/loader2.svg" } else { "titlebar/cloud.svg" }).size(px(14.0 * scale)).text_color(if failed { rgb(0xff9494).into() } else { appearance.chrome_icon });
                let glyph = if busy { icon.with_throttled_animation(format!("native-machine-busy-{id}"), std::time::Duration::from_millis(900), |icon, progress| icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(progress)))).into_any_element() } else { icon.into_any_element() };
                h_flex().id(format!("native-sidebar-machine-{id}")).relative().group(dot_group.clone()).role(gpui::Role::Tab).aria_label(machine.label.clone()).aria_selected(selected).flex_1().min_w_0().h_full().px(px(6.0 * scale)).justify_center().gap(px(4.0 * scale)).text_size(px(12.0 * scale)).text_color(appearance.chrome_label)
                    .on_prepaint(super::menus::record_menu_owner(&owner))
                    .when(index > 0, |row| row.border_l_1().border_color(appearance.foreground.opacity(0.12)))
                    .when(selected, |row| row.bg(appearance.selected)).hover(|row| row.bg(appearance.hover))
                    .child(div().id(format!("native-machine-connect-{id}")).flex_shrink_0().child(glyph)
                        .when(id != "local" && machine.state != "connected" && !busy, |icon| icon.on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.remote_reconnect_from_sidebar(&reconnect, cx); }))))
                    .child(div().min_w_0().truncate().child(machine.label.clone()))
                    .children(update_glyph)
                    /*
                    CDXC:Sidebar 2026-09-25 DECISION:
                    User: stop showing the status on the machine we're on and show it on the other machine tabs as dots, not numbers, exactly like the Spaces. The selected tab's sessions are already listed below it.
                    CDXC:Sidebar 2026-10-24 DECISION:
                    User: "when we show those colored circles on remote machines let's make those circles appear below the remote machine tab instead of showing in a way that pushes the icon and name of the remote/local machine". The dots are drawn absolutely at the bottom of the tab, centred, so the icon and name stay centred where they sit without dots and the tab does not grow; they are visual only. Supersedes the dots sitting inline after the name.
                    */
                    .when(!selected && (machine.working_count > 0 || machine.attention_count > 0 || machine.background_work_count > 0 || machine.question_count > 0 || machine.approval_count > 0), |row| row.child(h_flex().absolute().left_0().right_0().bottom(px(-4.0 * scale)).justify_center().child(super::status::status_dot_stack(machine.working_count, machine.attention_count, machine.background_work_count, machine.question_count, machine.approval_count, scale, super::status::DotBackdrop::new(appearance, None, dot_group.clone())))))
                    .when_some(tooltip.filter(|_| self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag()), |row, tooltip| row.tooltip_show_delay(appearance.tooltip_delay).tooltip(move |window, cx| super::tooltips::sidebar_style_tooltip(tooltip.clone(), window, cx)))
                    .on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.dispatch_native_sidebar_ui(json!({"type": "selectMachine", "machineId": id}), cx); }))
                    .when(machine.id != "local", |row| row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        cx.stop_propagation();
                        let mut items = Vec::new();
                        if let Some(label) = update_menu_label.clone() {
                            items.push(json!({"label": label, "icon": "download", "command": {"type": "remoteAppUpdate", "op": "install", "remoteMachineId": menu_id}}));
                            items.push(json!({"separator": true}));
                        }
                        items.push(json!({"label": "Hide Machine", "icon": "eye-off", "command": {"type": "machineAction", "action": "hide", "machineId": menu_id}}));
                        // The tab's workspace on this computer (gx-core `machine_workspace_menu`).
                        if let Some(workspace_menu) = workspace_menu.clone() {
                            items.push(workspace_menu);
                        }
                        items.push(json!({"separator": true}));
                        items.push(json!({"label": "Configure Machines", "icon": "settings", "command": {"type": "machineAction", "action": "configure", "machineId": menu_id}}));
                        let items = serde_json::Value::Array(items);
                        Self::show_native_sidebar_context_menu(&items, menu_owner.get(), event.position, scale, window, cx);
                    }))
            })).into_any_element())
    }
}

//! The searchable picker the Board opens from its project filter and from Add task's Project field
//! (a search field over All projects, the filter only, and the Space's projects, each with its
//! initial and folder) and from its Space caption (the machine's Spaces), the chosen one checked.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): the All projects filter and Add task's project can be searched, like PandaCode's project picker (a search field, All projects, then each project with its folder). "Tambahkan fitur untuk ganti space dari board view": the Space caption opens the same picker over the Spaces, and a pick selects that Space as the sidebar's Space row does, so the Board follows it.
//! User (Ryan): the picker was "transparent sehingga gak kelihatan" over window glass, so it is drawn on the menus' opaque fill (`titlebar_popup_menu_background`), as gpui-component's dropdowns beside it are, not the page's see-through panel tint.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, Pixels,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, anchored, deferred, div, point, px, rgb, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

use ghostex_gx_core::session_board::BoardFilter;

use super::view::NativeSessionBoardView;
use crate::app::helpers::{
    gpui_sidebar_command_icon_asset_path, titlebar_popup_menu_background,
    titlebar_popup_menu_border_color, titlebar_svg_icon,
};
use crate::app::native_chat::appearance::ChatAppearance;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::window::space_editor_modal::space_display_color;

const PICKER_WIDTH: f32 = 440.0;
const SPACE_PICKER_WIDTH: f32 = 280.0;
const ROW_HEIGHT: f32 = 34.0;
const LIST_MAX_HEIGHT: f32 = 320.0;
const ALL_PROJECTS: &str = "All projects";

/// Which control opened the picker, which decides its rows and what a pick changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickerTarget {
    /// The Board's project filter: All projects, then the Space's projects.
    Filter,
    /// Add task's Project field: the Space's projects.
    TaskProject,
    /// The Space caption: the machine's Spaces.
    Space,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PickerRow {
    All,
    Project(String),
    Space(String),
}

pub(super) struct ProjectPicker {
    target: PickerTarget,
    query: Entity<InputState>,
    highlight: usize,
    trigger: Bounds<Pixels>,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl ProjectPicker {
    pub(super) fn target(&self) -> PickerTarget {
        self.target
    }
}

impl NativeSessionBoardView {
    /// Opens the picker under `trigger`, or closes it when it is already open for `target`.
    pub(super) fn toggle_project_picker(
        &mut self,
        target: PickerTarget,
        trigger: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target == target)
        {
            self.picker = None;
            cx.notify();
            return;
        }
        let placeholder = if target == PickerTarget::Space {
            "Search spaces…"
        } else {
            "Search projects…"
        };
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(
            &query,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    if let Some(picker) = this.picker.as_mut() {
                        picker.highlight = 0;
                    }
                    cx.notify();
                }
                InputEvent::Focus => this.input_focused(cx),
                _ => {}
            },
        );
        query.update(cx, |input, cx| input.focus(window, cx));
        self.picker = Some(ProjectPicker {
            target,
            query,
            highlight: 0,
            trigger,
            scroll: ScrollHandle::new(),
            _subscription: subscription,
        });
        let rows = self.picker_rows(cx);
        let selected = rows.iter().position(|row| self.picker_row_checked(row));
        if let (Some(picker), Some(selected)) = (self.picker.as_mut(), selected) {
            picker.highlight = selected;
        }
        cx.notify();
    }

    pub(super) fn close_project_picker(&mut self, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            cx.notify();
        }
    }

    fn picker_rows(&self, cx: &gpui::App) -> Vec<PickerRow> {
        let Some(picker) = self.picker.as_ref() else {
            return Vec::new();
        };
        let query = picker.query.read(cx).value().to_string();
        let needle = query.trim().to_lowercase();
        if picker.target == PickerTarget::Space {
            return self
                .board
                .spaces
                .iter()
                .filter(|space| needle.is_empty() || space.name.to_lowercase().contains(&needle))
                .map(|space| PickerRow::Space(space.space_id.clone()))
                .collect();
        }
        let mut rows = Vec::new();
        if picker.target == PickerTarget::Filter
            && (needle.is_empty() || ALL_PROJECTS.to_lowercase().contains(&needle))
        {
            rows.push(PickerRow::All);
        }
        rows.extend(
            self.board
                .projects
                .iter()
                .filter(|project| project.matches(&query))
                .map(|project| PickerRow::Project(project.project_id.clone())),
        );
        rows
    }

    fn picker_row_checked(&self, row: &PickerRow) -> bool {
        let Some(picker) = self.picker.as_ref() else {
            return false;
        };
        match (picker.target, row) {
            (PickerTarget::Filter, PickerRow::All) => *self.filter() == BoardFilter::All,
            (PickerTarget::Filter, PickerRow::Project(id)) => {
                *self.filter() == BoardFilter::Project(id.clone())
            }
            (PickerTarget::TaskProject, PickerRow::Project(id)) => self
                .panel
                .as_ref()
                .and_then(|panel| panel.project_id())
                .is_some_and(|chosen| chosen == id),
            (_, PickerRow::Space(id)) => self
                .board
                .spaces
                .iter()
                .any(|space| &space.space_id == id && space.selected),
            (PickerTarget::TaskProject | PickerTarget::Space, PickerRow::All)
            | (PickerTarget::Space, PickerRow::Project(_)) => false,
        }
    }

    fn choose_picker_row(&mut self, row: PickerRow, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        match (picker.target, row) {
            (PickerTarget::Filter, PickerRow::All) => {
                self.set_filter(BoardFilter::All, window, cx)
            }
            (PickerTarget::Filter, PickerRow::Project(id)) => {
                self.set_filter(BoardFilter::Project(id), window, cx)
            }
            (PickerTarget::TaskProject, PickerRow::Project(id)) => {
                if let Some(panel) = self.panel.as_mut() {
                    panel.set_project_id(id);
                }
            }
            (_, PickerRow::Space(id)) => {
                self.app_later(window, cx, move |app, _, cx| app.select_native_space(&id, cx));
            }
            (PickerTarget::TaskProject | PickerTarget::Space, PickerRow::All)
            | (PickerTarget::Space, PickerRow::Project(_)) => {}
        }
        cx.notify();
    }

    fn picker_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.picker_rows(cx);
        let Some(highlight) = self.picker.as_ref().map(|picker| picker.highlight) else {
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                self.close_project_picker(cx);
            }
            "up" | "down" if !rows.is_empty() => {
                cx.stop_propagation();
                let delta: isize = if event.keystroke.key == "up" { -1 } else { 1 };
                let next = (highlight as isize + delta).rem_euclid(rows.len() as isize) as usize;
                if let Some(picker) = self.picker.as_mut() {
                    picker.highlight = next;
                    picker.scroll.scroll_to_item(next);
                }
                cx.notify();
            }
            "enter" => {
                cx.stop_propagation();
                if let Some(row) = rows.get(highlight).cloned() {
                    self.choose_picker_row(row, window, cx);
                }
            }
            _ => {}
        }
    }

    fn render_picker_row(
        &self,
        index: usize,
        row: &PickerRow,
        highlighted: bool,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self.picker_row_checked(row);
        let (title, path): (SharedString, Option<SharedString>) = match row {
            PickerRow::All => (ALL_PROJECTS.into(), None),
            PickerRow::Project(id) => {
                let project = self
                    .board
                    .projects
                    .iter()
                    .find(|project| &project.project_id == id);
                (
                    project
                        .map(|project| project.label())
                        .unwrap_or_else(|| id.clone())
                        .into(),
                    project
                        .and_then(|project| project.path.clone())
                        .map(SharedString::from),
                )
            }
            PickerRow::Space(id) => (
                self.board
                    .spaces
                    .iter()
                    .find(|space| &space.space_id == id)
                    .map_or_else(|| id.clone(), |space| space.name.clone())
                    .into(),
                None,
            ),
        };
        let leading = match row {
            PickerRow::All => titlebar_svg_icon("titlebar/folder.svg", 15.0, p.muted).into_any_element(),
            PickerRow::Space(id) => {
                let light = ChatAppearance::current(&serde_json::Value::Null).light;
                let space = self.board.spaces.iter().find(|space| &space.space_id == id);
                let color = space
                    .and_then(|space| {
                        let shown = space_display_color(&space.color, light);
                        u32::from_str_radix(shown.trim_start_matches('#'), 16).ok()
                    })
                    .map_or(p.muted, |value| rgb(value).into());
                svg()
                    .path(gpui_sidebar_command_icon_asset_path(
                        space.map(|space| space.icon.as_str()),
                    ))
                    .size(px(15.0))
                    .flex_none()
                    .text_color(color)
                    .into_any_element()
            }
            PickerRow::Project(_) => div()
                .size(px(18.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .bg(p.control_hover)
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(
                    title
                        .chars()
                        .find(|character| character.is_alphanumeric())
                        .map(|character| character.to_uppercase().to_string())
                        .unwrap_or_default(),
                )
                .into_any_element(),
        };
        let has_path = path.is_some();
        let hover = p.control_hover;
        let chosen = row.clone();
        h_flex()
            .id(("session-board-picker-row", index))
            .w_full()
            .min_w_0()
            .h(px(ROW_HEIGHT))
            .flex_none()
            .px(px(10.0))
            .gap(px(10.0))
            .items_center()
            .rounded(px(7.0))
            .cursor_pointer()
            .when(highlighted, |this| this.bg(hover))
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose_picker_row(chosen.clone(), window, cx);
            }))
            .child(leading)
            .child(
                div()
                    .flex_none()
                    .when(has_path, |this| this.max_w(gpui::relative(0.45)))
                    .truncate()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.foreground)
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .text_color(p.faint)
                    .children(path),
            )
            .child(
                div()
                    .flex_none()
                    .when(!checked, |this| this.opacity(0.0))
                    .child(titlebar_svg_icon("titlebar/check.svg", 15.0, p.accent)),
            )
            .into_any_element()
    }

    /// The picker, drawn over the page while it is open.
    pub(super) fn render_project_picker(
        &self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.picker.as_ref()?;
        let rows = self.picker_rows(cx);
        let highlight = picker.highlight.min(rows.len().saturating_sub(1));
        let trigger = picker.trigger;
        let width = px(if picker.target == PickerTarget::Space {
            SPACE_PICKER_WIDTH
        } else {
            PICKER_WIDTH
        })
        .max(trigger.size.width);
        let list: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.render_picker_row(index, row, index == highlight, p, cx))
            .collect();
        let empty = rows.is_empty();
        let popup = v_flex()
            .id("session-board-project-picker")
            .occlude()
            .w(width)
            .rounded(px(10.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(titlebar_popup_menu_background())
            .shadow_lg()
            .font_family(p.font.clone())
            .text_color(p.foreground)
            .on_key_down(cx.listener(Self::picker_key_down))
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if !trigger.contains(&event.position) {
                    this.close_project_picker(cx);
                }
            }))
            .child(
                h_flex()
                    .flex_none()
                    .h(px(44.0))
                    .px(px(14.0))
                    .gap(px(10.0))
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(titlebar_svg_icon("titlebar/search.svg", 15.0, p.muted))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&picker.query)
                                .with_size(ComponentSize::Small)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .px(px(0.0))
                                .py(px(0.0))
                                .text_size(px(14.0))
                                .text_color(p.foreground),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .id("session-board-project-picker-list")
                    .w_full()
                    .max_h(px(LIST_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&picker.scroll)
                    .p(px(6.0))
                    .gap(px(2.0))
                    .when(empty, |this| {
                        this.child(
                            div()
                                .py(px(18.0))
                                .text_center()
                                .text_size(px(12.5))
                                .text_color(p.faint)
                                .child(if picker.target == PickerTarget::Space {
                                    "No space matches."
                                } else {
                                    "No project matches."
                                }),
                        )
                    })
                    .children(list),
            );
        Some(
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Window)
                    .position(point(
                        trigger.origin.x,
                        trigger.origin.y + trigger.size.height + px(6.0),
                    ))
                    .snap_to_window_with_margin(px(8.0))
                    .child(popup),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

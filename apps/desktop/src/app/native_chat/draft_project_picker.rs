//! The tray's project chip and what it opens: a search over every project on every machine, one
//! row per repository with a chip per machine. The rows are gx-core's
//! (`unified_sidebar/choices.rs`); picking moves the draft (`gx_store/unified_new_session.rs`). The
//! phone draws the same choice in `apps/mobile/app/src/chat/native/composer/ProjectPickerSheet.tsx`.
//!
//! CDXC:Worktrees 2026-10-31 DECISION:
//! User (Q847): "we need to add picking the project and machine in the bar that we added today where we added the ability to pick either the project folder or the worktree". The chip is the tray's first control; it reads the project, and the machine's name when the chat is on another machine. User (Q848): one row per repository with machine chips, stays at 100 rows; left and right arrows change the machine. The search opens on the draft's own project, so Escape leaves things as they were.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::gx_store::unified_new_session::PendingDraftMove;
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_core::unified_sidebar::{
    UnifiedProjectChoice, filter_project_choices, move_project_choice_machine,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, ScrollHandle, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, deferred, div, px, svg,
};
use gpui_component::ElementExt as _;
use gpui_component::input::{Input, InputEvent, InputState};

/// The open search.
pub(crate) struct DraftProjectPicker {
    choices: Vec<UnifiedProjectChoice>,
    input: Entity<InputState>,
    query: String,
    highlighted: usize,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl DraftProjectPicker {
    fn rows(&self) -> Vec<UnifiedProjectChoice> {
        filter_project_choices(&self.choices, &self.query)
    }
}

impl NativeChatView {
    /// The search's field while it is open, which takes the chat's typing (edit_shortcuts.rs).
    pub(super) fn draft_project_picker_input(&self) -> Option<&Entity<InputState>> {
        self.draft_project_picker
            .as_ref()
            .map(|picker| &picker.input)
    }

    /// What the chip reads.
    pub(super) fn draft_project_chip_label(&self, name: &str, cx: &Context<Self>) -> String {
        let machine = (self.config.machine_id != ghostex_gx_core::LOCAL_MACHINE_ID)
            .then(|| {
                let app = self.config.app.as_ref()?.upgrade()?;
                let inputs = &app.read(cx).gx_store.sidebar_list.last_inputs;
                inputs
                    .host
                    .machines
                    .iter()
                    .find(|tab| tab.machine_id == self.config.machine_id)
                    .map(|tab| tab.label.clone())
            })
            .flatten();
        match machine {
            Some(machine) => format!("{name} · {machine}"),
            None => name.to_string(),
        }
    }

    pub(super) fn render_draft_project_chip(
        &self,
        name: &str,
        p: &ChatAppearance,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let label = self.draft_project_chip_label(name, cx);
        let bounds = self.draft_project_chip_bounds.clone();
        div()
            .on_prepaint(move |painted, _, _| bounds.set(painted))
            .id("chat-draft-project")
            .role(gpui::Role::Button)
            .aria_label(format!("Project: {label}"))
            .flex_shrink_0()
            .h(px(26.0 * s))
            .px(px(8.0 * s))
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .rounded(px(7.0 * s))
            .border_1()
            .border_color(p.composer_border)
            .bg(p.composer_background)
            .text_color(p.primary)
            .when(!busy, |chip| {
                chip.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border))
                    .on_click(cx.listener(|chat, _, window, cx| {
                        chat.toggle_draft_project_picker(window, cx);
                    }))
            })
            .child(
                svg()
                    .path("titlebar/folder.svg")
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .max_w(px(180.0 * s))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    fn toggle_draft_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft_project_picker.take().is_some() {
            cx.notify();
            return;
        }
        let Some(app) = self.config.app.as_ref().and_then(|app| app.upgrade()) else {
            return;
        };
        let choices = app.read(cx).gx_store_project_choices();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                |chat, input, event, window, cx| match event {
                    InputEvent::Change => {
                        let query = input.read(cx).value().to_string();
                        if let Some(picker) = &mut chat.draft_project_picker {
                            picker.query = query;
                            picker.highlighted = 0;
                            cx.notify();
                        }
                    }
                    InputEvent::PressEnter { .. } => chat.pick_highlighted_project(window, cx),
                    _ => {}
                },
            );
        let project_id = self.config.project_id.clone();
        let highlighted = choices
            .iter()
            .position(|choice| {
                choice
                    .machines
                    .iter()
                    .any(|machine| machine.project_id == project_id)
            })
            .unwrap_or(0);
        input.update(cx, |input, cx| input.focus(window, cx));
        let scroll = ScrollHandle::new();
        scroll.scroll_to_item(highlighted);
        self.draft_project_picker = Some(DraftProjectPicker {
            choices,
            input,
            query: String::new(),
            highlighted,
            scroll,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn close_draft_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft_project_picker = None;
        self.focus_composer_from_hotkey(window, cx);
        cx.notify();
    }

    /// Up and down walk the rows; left and right change the highlighted row's machine.
    fn draft_project_picker_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if key == "escape" {
            self.close_draft_project_picker(window, cx);
            return true;
        }
        let Some(picker) = &mut self.draft_project_picker else {
            return false;
        };
        let rows = picker.rows();
        let Some(row) = rows.get(picker.highlighted) else {
            return false;
        };
        match key {
            "up" => picker.highlighted = picker.highlighted.saturating_sub(1),
            "down" => picker.highlighted = (picker.highlighted + 1).min(rows.len() - 1),
            "left" | "right" if row.machines.len() > 1 => {
                let selected = move_project_choice_machine(row, key == "right");
                let repo_key = row.repo_key.clone();
                if let Some(choice) = picker
                    .choices
                    .iter_mut()
                    .find(|choice| choice.repo_key == repo_key)
                {
                    choice.selected = selected;
                }
            }
            _ => return false,
        }
        picker.scroll.scroll_to_item(picker.highlighted);
        cx.notify();
        true
    }

    /// The composer's key handler, for keys that reach it while the search is open.
    pub(super) fn draft_project_picker_key_down(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.draft_project_picker.is_none() {
            return false;
        }
        if key == "enter" {
            self.pick_highlighted_project(window, cx);
            return true;
        }
        self.draft_project_picker_key(key, window, cx)
    }

    fn pick_highlighted_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &self.draft_project_picker else {
            return;
        };
        let rows = picker.rows();
        let Some(row) = rows.get(picker.highlighted) else {
            return;
        };
        self.pick_draft_project(row, row.selected, window, cx);
    }

    fn pick_draft_project(
        &mut self,
        row: &UnifiedProjectChoice,
        chip: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(machine) = row.machines.get(chip) else {
            return;
        };
        // A machine that is away can't start anything yet; its chip is dimmed.
        if !machine.connected {
            return;
        }
        let same = machine.project_id == self.config.project_id
            && machine.machine_id == self.config.machine_id;
        let agent_id = self.snapshot["sessionAgentId"].as_str().map(str::to_string);
        self.close_draft_project_picker(window, cx);
        let (false, Some(agent_id), Some(app)) = (
            same,
            agent_id,
            self.config.app.as_ref().and_then(|app| app.upgrade()),
        ) else {
            return;
        };
        let draft = PendingDraftMove {
            from_machine_id: self.config.machine_id.clone(),
            from_project_id: self.config.project_id.clone(),
            from_session_id: self.config.session_id.clone(),
            text: self.draft.clone(),
        };
        let (repo_key, machine_id, project_id) = (
            row.repo_key.clone(),
            machine.machine_id.clone(),
            machine.project_id.clone(),
        );
        cx.defer(move |cx| {
            app.update(cx, |app, cx| {
                app.gx_store_move_draft_to_project(
                    draft,
                    &agent_id,
                    &repo_key,
                    &machine_id,
                    &project_id,
                    cx,
                );
            });
        });
    }

    /// The search, above the tray.
    pub(super) fn render_draft_project_picker(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.draft_project_picker.as_ref()?;
        let s = p.scale;
        let rows = picker.rows();
        let highlighted = picker.highlighted;
        let rows_view: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let on = index == highlighted;
                let pick_row = row.clone();
                let chips = row
                    .machines
                    .len()
                    .gt(&1)
                    .then_some(())
                    .or_else(|| {
                        (row.machines
                            .first()
                            .map(|machine| machine.machine_id.as_str())
                            != Some(ghostex_gx_core::LOCAL_MACHINE_ID))
                        .then_some(())
                    })
                    .map(|_| {
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(3.0 * s))
                            .children(row.machines.iter().enumerate().map(|(chip, machine)| {
                                let selected = chip == row.selected;
                                let chip_row = row.clone();
                                div()
                                    .id(format!("chat-draft-project-{index}-chip-{chip}"))
                                    .role(gpui::Role::Button)
                                    .aria_label(machine.label.clone())
                                    .h(px(20.0 * s))
                                    .px(px(6.0 * s))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.0 * s))
                                    .text_size(px(11.5 * s))
                                    .border_1()
                                    .border_color(if selected {
                                        p.muted
                                    } else {
                                        p.composer_border
                                    })
                                    .text_color(if selected { p.primary } else { p.muted })
                                    .when(!machine.connected, |chip| chip.opacity(0.45))
                                    .when(machine.connected, |chip| chip.chat_cursor_pointer())
                                    .child(machine.label.clone())
                                    .on_click(cx.listener(move |chat, _, window, cx| {
                                        cx.stop_propagation();
                                        chat.pick_draft_project(&chip_row, chip, window, cx);
                                    }))
                            }))
                    });
                div()
                    .id(("chat-draft-project-row", index))
                    .role(gpui::Role::Button)
                    .aria_label(row.label.clone())
                    .min_h(px(30.0 * s))
                    .px(px(8.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .rounded(px(6.0 * s))
                    .text_size(px(13.0 * s))
                    .text_color(p.primary)
                    .chat_cursor_pointer()
                    .when(on, |row| row.bg(p.border))
                    .hover(|style| style.bg(p.border))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(row.label.clone()),
                    )
                    .children(chips)
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        cx.stop_propagation();
                        chat.pick_draft_project(&pick_row, pick_row.selected, window, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        Some(
            deferred(
                div()
                    .id("chat-draft-project-picker")
                    .absolute()
                    .bottom(px(40.0 * s))
                    .left(px(8.0 * s))
                    .w(px(380.0 * s))
                    .flex()
                    .flex_col()
                    .rounded(px(10.0 * s))
                    .border_1()
                    .border_color(p.composer_border)
                    .bg(p.composer_background)
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|chat, event: &gpui::MouseDownEvent, _, cx| {
                        // A press on the chip is its own toggle, which closes it on release.
                        if !chat
                            .draft_project_chip_bounds
                            .get()
                            .contains(&event.position)
                        {
                            chat.draft_project_picker = None;
                            cx.notify();
                        }
                    }))
                    .capture_key_down(cx.listener(
                        |chat, event: &gpui::KeyDownEvent, window, cx| {
                            if chat.draft_project_picker_key(&event.keystroke.key, window, cx) {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .child(
                        div()
                            .h(px(34.0 * s))
                            .px(px(10.0 * s))
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .border_b_1()
                            .border_color(p.composer_border)
                            .child(
                                svg()
                                    .path("titlebar/search.svg")
                                    .size(px(13.0 * s))
                                    .flex_shrink_0()
                                    .text_color(p.muted),
                            )
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&picker.input)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .p_0()
                                        .text_size(px(13.0 * s))
                                        .text_color(p.primary)
                                        .placeholder_color(p.muted.opacity(0.6)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .id("chat-draft-project-rows")
                            .flex()
                            .flex_col()
                            .max_h(px(280.0 * s))
                            .p(px(4.0 * s))
                            .overflow_y_scroll()
                            .track_scroll(&picker.scroll)
                            .children(rows_view)
                            .when(empty, |list| {
                                list.child(
                                    div()
                                        .px(px(8.0 * s))
                                        .py(px(8.0 * s))
                                        .text_size(px(13.0 * s))
                                        .text_color(p.muted)
                                        .child("No projects match"),
                                )
                            }),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

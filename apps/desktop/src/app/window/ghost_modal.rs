//! The Ghost modal: New Ghost (name, look, agent, projects), and a Ghost's settings in four tabs.
//! General edits the name, the look and the projects it holds, and offers Reset conversation and
//! Delete; Soul and Memory edit SOUL.md and MEMORY.md as text; Skills adds and removes the Ghost's
//! own skills. Every save waits for gxserver and keeps the edit with the error inline when it is
//! refused; only Cancel and the corner close the window.
//!
//! CDXC:Ghosts 2026-10-29 WHY:
//! Soul and Memory save with the hash they were read with (`/api/writeGhostFile`), so an edit made here never overwrites a fact the Ghost saved meanwhile: the save is refused with a message to reopen instead.
//! SEE-ALSO: apps/desktop/src/app/native_ghost/modal_host.rs (open, the RPCs), server/src/ghosts/endpoint.rs.
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::input::{Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::rc::Rc;

pub(crate) const GHOST_MODAL_WIDTH: f32 = 640.0;
pub(crate) const GHOST_MODAL_INITIAL_HEIGHT: f32 = 760.0;

/// The colours a Ghost can wear (native_sidebar/ghosts.rs draws them).
const GHOST_COLOR_CHOICES: [&str; 8] = [
    "#7c9cff", "#4fd1a5", "#f59e0b", "#f472b6", "#a78bfa", "#38bdf8", "#fb7185", "#94a3b8",
];

#[derive(Clone, Debug)]
pub(crate) struct GhostModalAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct GhostModalProject {
    pub(crate) project_id: String,
    pub(crate) name: String,
    /// The sidebar's project group it sits in; empty when it is in none.
    pub(crate) group: String,
    pub(crate) group_color: String,
}

/// Where a save's outcome goes: the modal that sent it, in its own window.
pub(crate) struct GhostModalReply {
    view: WeakEntity<GpuiGhostModalWindow>,
    window: AnyWindowHandle,
}

impl GhostModalReply {
    /// Hands gxserver's answer to the modal. False when the modal is gone.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) -> bool {
        let view = self.view;
        self.window
            .update(cx, |_root, window, cx| {
                view.update(cx, |modal, cx| modal.finish(result, window, cx))
            })
            .is_ok_and(|delivered| delivered.is_ok())
    }
}

pub(crate) enum GhostModalCommand {
    Create {
        name: String,
        mascot: String,
        color: String,
        agent_id: String,
        project_ids: Vec<String>,
        reply: GhostModalReply,
    },
    SaveGeneral {
        name: String,
        mascot: String,
        color: String,
        project_ids: Vec<String>,
        reply: GhostModalReply,
    },
    SaveFile {
        file: &'static str,
        text: String,
        expected_hash: String,
        reply: GhostModalReply,
    },
    /// `add` (an installed skill by name) or `remove`; the answer is the new skill lists.
    Skill {
        action: &'static str,
        name: String,
        reply: GhostModalReply,
    },
    Reset {
        reply: GhostModalReply,
    },
    Delete {
        reply: GhostModalReply,
    },
    Cancel,
}

pub(crate) type GhostModalHost = Rc<dyn Fn(GhostModalCommand, &mut App)>;

pub(crate) struct GhostModalConfig {
    /// The Ghost as `/api/readGhost` answered (with its Soul and Memory); `None` for New Ghost.
    pub(crate) ghost: Option<Value>,
    /// `/api/ghostSkills` (`skills`, `available`) for a Ghost's settings.
    pub(crate) skills: Value,
    pub(crate) agents: Vec<GhostModalAgent>,
    pub(crate) projects: Vec<GhostModalProject>,
    pub(crate) palette: ModalPalette,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Soul,
    Memory,
    Skills,
}

const TABS: [Tab; 4] = [Tab::General, Tab::Soul, Tab::Memory, Tab::Skills];

/// What the pending save was, so its answer lands in the right place.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Create,
    General,
    Soul,
    Memory,
    Skill,
    Reset,
    Delete,
}

pub(crate) struct GpuiGhostModalWindow {
    host: GhostModalHost,
    palette: ModalPalette,
    is_new: bool,
    tab: Tab,
    name_input: Entity<InputState>,
    mascot_input: Entity<InputState>,
    project_search_input: Entity<InputState>,
    soul_input: Entity<TextareaState>,
    memory_input: Entity<TextareaState>,
    color: String,
    agents: Vec<GhostModalAgent>,
    agent: usize,
    projects: Vec<GhostModalProject>,
    selected_projects: Vec<String>,
    soul_hash: String,
    memory_hash: String,
    skills: Value,
    pending: Option<Pending>,
    /// Asked once more before Delete and Reset go through.
    confirming: Option<Pending>,
    error: Option<String>,
    notice: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl GpuiGhostModalWindow {
    pub(crate) fn new(
        config: GhostModalConfig,
        host: GhostModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let ghost = config.ghost.clone().unwrap_or(Value::Null);
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Dianah")
                .default_value(text(&ghost, "name"))
        });
        let mascot_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("An emoji, or leave empty for its initial")
                .default_value(text(&ghost, "mascot"))
        });
        let project_search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search projects or groups")
        });
        let soul_input = cx.new(|cx| {
            TextareaState::new(window, cx).default_value(text(&ghost["soul"], "text"))
        });
        let memory_input = cx.new(|cx| {
            TextareaState::new(window, cx).default_value(text(&ghost["memory"], "text"))
        });
        let notify_on_change = |cx: &mut Context<Self>, input: &Entity<InputState>, window: &mut Window| {
            cx.subscribe_in(input, window, |_: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
        };
        let subscriptions = vec![
            notify_on_change(cx, &name_input, window),
            notify_on_change(cx, &mascot_input, window),
            notify_on_change(cx, &project_search_input, window),
        ];
        name_input.update(cx, |input, cx| input.focus(window, cx));
        let selected_projects = ghost["projectIds"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            host,
            palette: config.palette,
            is_new: config.ghost.is_none(),
            tab: Tab::General,
            name_input,
            mascot_input,
            project_search_input,
            soul_input,
            memory_input,
            color: text(&ghost, "color"),
            agents: config.agents,
            agent: 0,
            projects: config.projects,
            selected_projects,
            soul_hash: text(&ghost["soul"], "hash"),
            memory_hash: text(&ghost["memory"], "hash"),
            skills: config.skills,
            pending: None,
            confirming: None,
            error: None,
            notice: None,
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    fn reply(&self, window: &Window, cx: &Context<Self>) -> GhostModalReply {
        GhostModalReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        }
    }

    fn send(&mut self, pending: Pending, command: GhostModalCommand, cx: &mut Context<Self>) {
        self.pending = Some(pending);
        self.confirming = None;
        self.error = None;
        self.notice = None;
        cx.notify();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(GhostModalCommand::Cancel, cx);
    }

    /// gxserver's answer to the pending save.
    fn finish(&mut self, result: Result<Value, String>, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self.pending.take();
        match result {
            Err(message) => self.error = Some(message),
            Ok(value) => match pending {
                Some(Pending::Create | Pending::Delete) => {
                    window.remove_window();
                    return;
                }
                Some(Pending::Soul) => {
                    self.soul_hash = text(&value, "hash");
                    self.notice = Some("Soul saved. The Ghost reads it at its next start.".into());
                }
                Some(Pending::Memory) => {
                    self.memory_hash = text(&value, "hash");
                    self.notice = Some("Memory saved.".into());
                }
                Some(Pending::Skill) => self.skills = value,
                Some(Pending::General) => self.notice = Some("Saved.".into()),
                Some(Pending::Reset) => {
                    self.notice = Some("The Ghost starts a new conversation; its Soul and Memory stay.".into())
                }
                None => {}
            },
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        let reply = self.reply(window, cx);
        match self.tab {
            Tab::General => {
                let name = self.name_input.read(cx).value().trim().to_string();
                if name.is_empty() {
                    self.error = Some("Give the Ghost a name.".into());
                    cx.notify();
                    return;
                }
                let mascot = self.mascot_input.read(cx).value().trim().to_string();
                let (color, project_ids) = (self.color.clone(), self.selected_projects.clone());
                if self.is_new {
                    let Some(agent) = self.agents.get(self.agent) else {
                        self.error = Some("No agent a Ghost can run on is configured.".into());
                        cx.notify();
                        return;
                    };
                    let agent_id = agent.agent_id.clone();
                    self.send(
                        Pending::Create,
                        GhostModalCommand::Create { name, mascot, color, agent_id, project_ids, reply },
                        cx,
                    );
                } else {
                    self.send(
                        Pending::General,
                        GhostModalCommand::SaveGeneral { name, mascot, color, project_ids, reply },
                        cx,
                    );
                }
            }
            Tab::Soul => {
                let text = self.soul_input.read(cx).value().to_string();
                let expected_hash = self.soul_hash.clone();
                self.send(
                    Pending::Soul,
                    GhostModalCommand::SaveFile { file: "SOUL.md", text, expected_hash, reply },
                    cx,
                );
            }
            Tab::Memory => {
                let text = self.memory_input.read(cx).value().to_string();
                let expected_hash = self.memory_hash.clone();
                self.send(
                    Pending::Memory,
                    GhostModalCommand::SaveFile { file: "MEMORY.md", text, expected_hash, reply },
                    cx,
                );
            }
            Tab::Skills => {}
        }
    }

    fn skill(&mut self, action: &'static str, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        let reply = self.reply(window, cx);
        self.send(Pending::Skill, GhostModalCommand::Skill { action, name, reply }, cx);
    }

    fn danger(&mut self, which: Pending, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        if self.confirming != Some(which) {
            self.confirming = Some(which);
            cx.notify();
            return;
        }
        let reply = self.reply(window, cx);
        let command = if which == Pending::Delete {
            GhostModalCommand::Delete { reply }
        } else {
            GhostModalCommand::Reset { reply }
        };
        self.send(which, command, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.cancel(window, cx);
            cx.stop_propagation();
        }
    }

    fn render_general(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut colors = h_flex().gap(px(8.0));
        for (index, choice) in GHOST_COLOR_CHOICES.iter().enumerate() {
            let hex = u32::from_str_radix(choice.trim_start_matches('#'), 16).unwrap_or(0x94a3b8);
            let selected = self.color.eq_ignore_ascii_case(choice);
            colors = colors.child(
                div()
                    .id(SharedString::from(format!("ghost-color-{index}")))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(format!("Colour {choice}")))
                    .aria_selected(selected)
                    .size(px(22.0))
                    .rounded_full()
                    .bg(gpui::rgb(hex))
                    .cursor_pointer()
                    .border_2()
                    .border_color(if selected { hsla(p.foreground) } else { gpui::transparent_black() })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.color = choice.to_string();
                        cx.notify();
                    })),
            );
        }
        // The list takes the room the frame has left and scrolls on its own; the shell never
        // scrolls around it as well.
        let mut projects = v_flex()
            .id("ghost-projects")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(2.0));
        let query = self.project_search_input.read(cx).value().trim().to_lowercase();
        let shown: Vec<&GhostModalProject> = self
            .projects
            .iter()
            .filter(|project| {
                query.is_empty()
                    || format!("{} - {}", project.group, project.name)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect();
        if self.projects.is_empty() {
            projects = projects.child(modal_hint(&p, "No projects yet. Add one to Ghostex first."));
        } else if shown.is_empty() {
            projects = projects.child(modal_hint(&p, format!("No project or group matches \u{201c}{query}\u{201d}.")));
        }
        let grouped = self.projects.iter().any(|project| !project.group.is_empty());
        for project in shown {
            let color = u32::from_str_radix(project.group_color.trim_start_matches('#'), 16).ok();
            // "Group - Project", as the sidebar nests it; a project in no group shows its name.
            let mut label = h_flex().flex_1().min_w_0().gap(px(7.0)).text_size(px(13.5));
            if grouped {
                label = label.child(
                    div()
                        .size(px(7.0))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(color.map_or(hsla(p.muted).opacity(0.35), |hex| gpui::rgb(hex).into())),
                );
            }
            if !project.group.is_empty() {
                label = label.child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.muted)
                        .child(format!("{} -", project.group)),
                );
            }
            label = label.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.foreground)
                    .child(project.name.clone()),
            );
            let checked = self.selected_projects.contains(&project.project_id);
            let project_id = project.project_id.clone();
            projects = projects.child(
                h_flex()
                    .id(SharedString::from(format!("ghost-project-{}", project.project_id)))
                    .w_full()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.raised_hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(index) =
                            this.selected_projects.iter().position(|id| *id == project_id)
                        {
                            this.selected_projects.remove(index);
                        } else {
                            this.selected_projects.push(project_id.clone());
                        }
                        cx.notify();
                    }))
                    .child(label)
                    .child(modal_switch(&p, checked, false)),
            );
        }
        let mut body = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(14.0))
            .child(
                v_flex()
                    .gap(px(6.0))
                    .child(modal_section_title(&p, "Name"))
                    .child(modal_text_input(&p, &self.name_input, false, window, cx)),
            )
            .child(
                v_flex()
                    .gap(px(6.0))
                    .child(modal_section_title(&p, "Look"))
                    .child(modal_text_input(&p, &self.mascot_input, false, window, cx))
                    .child(colors),
            );
        if self.is_new && !self.agents.is_empty() {
            let mut agents = h_flex().gap(px(6.0)).flex_wrap();
            for (index, agent) in self.agents.iter().enumerate() {
                let selected = index == self.agent;
                agents = agents.child(
                    div()
                        .id(SharedString::from(format!("ghost-agent-{index}")))
                        .role(gpui::Role::Button)
                        .aria_selected(selected)
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .border_1()
                        .border_color(hsla(if selected { p.focus_border } else { p.hairline }))
                        .when(selected, |this| this.bg(hsla(p.raised)))
                        .text_size(px(13.0))
                        .text_color(p.foreground)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.agent = index;
                            cx.notify();
                        }))
                        .child(agent.name.clone()),
                );
            }
            body = body.child(
                v_flex()
                    .gap(px(6.0))
                    .child(modal_section_title(&p, "Agent"))
                    .child(agents)
                    .child(modal_hint(&p, "What it thinks and replies with. Its model and effort follow that agent's settings.")),
            );
        }
        let projects_title = match self.selected_projects.len() {
            0 => "Projects".to_string(),
            count => format!("Projects \u{b7} {count} selected"),
        };
        body = body.child(
            v_flex()
                .flex_1()
                .min_h_0()
                .gap(px(6.0))
                .child(modal_section_title(&p, projects_title))
                .child(modal_hint(&p, "It can open threads in these, and sees their sessions and files."))
                .when(!self.projects.is_empty(), |this| {
                    this.child(modal_text_input(&p, &self.project_search_input, false, window, cx))
                })
                .child(projects),
        );
        if !self.is_new {
            let confirm = |which| self.confirming == Some(which);
            body = body.child(
                v_flex()
                    .gap(px(6.0))
                    .child(modal_section_title(&p, "Conversation"))
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .child(modal_action_button(
                                &p,
                                "ghost-reset",
                                if confirm(Pending::Reset) { "Click again to reset" } else { "Reset conversation" },
                                None,
                                ModalButtonTone::Neutral,
                                self.pending.is_some(),
                                |this, window, cx| this.danger(Pending::Reset, window, cx),
                                cx,
                            ))
                            .child(modal_action_button(
                                &p,
                                "ghost-delete",
                                if confirm(Pending::Delete) { "Click again to delete" } else { "Delete Ghost" },
                                None,
                                ModalButtonTone::Danger,
                                self.pending.is_some(),
                                |this, window, cx| this.danger(Pending::Delete, window, cx),
                                cx,
                            )),
                    )
                    .child(modal_hint(&p, "Delete removes the Ghost, its chat and its habits. Its Soul, Memory and documents stay in its folder.")),
            );
        }
        body.into_any_element()
    }

    fn render_file(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (state, hint) = if self.tab == Tab::Soul {
            (
                &self.soul_input,
                "Who the Ghost is and its standing instructions. It reads this at every start.",
            )
        } else {
            (
                &self.memory_input,
                "Dated facts the Ghost keeps (- date · fact). Lines without a date are yours and are never changed by the Ghost.",
            )
        };
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(8.0))
            .child(modal_hint(&p, hint))
            .child(modal_text_area(&p, state, Some(380.0), self.pending.is_some(), window, cx))
            .into_any_element()
    }

    fn render_skills(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let row = |name: String, description: String, label: &'static str, action: &'static str, index: usize, cx: &mut Context<Self>| {
            h_flex()
                .w_full()
                .gap(px(10.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(hsla(p.raised))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_size(px(13.5)).text_color(p.foreground).child(name.clone()))
                        .when(!description.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.muted)
                                    .truncate()
                                    .child(description),
                            )
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("ghost-skill-{action}-{index}")))
                        .role(gpui::Role::Button)
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .text_color(p.foreground)
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .hover(move |this| this.bg(hsla(p.raised_hover)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.skill(action, name.clone(), window, cx)
                        }))
                        .child(label),
                )
        };
        let mut own = v_flex().gap(px(4.0));
        let own_rows = self.skills["skills"].as_array().cloned().unwrap_or_default();
        if own_rows.is_empty() {
            own = own.child(modal_hint(&p, "None yet. Add one of your installed skills below; only this Ghost will see its copy."));
        }
        for (index, skill) in own_rows.iter().enumerate() {
            own = own.child(row(text(skill, "name"), text(skill, "description"), "Remove", "remove", index, cx));
        }
        let mut available = v_flex().gap(px(4.0));
        for (index, skill) in self.skills["available"].as_array().into_iter().flatten().enumerate() {
            available = available.child(row(text(skill, "name"), text(skill, "description"), "Add", "add", index, cx));
        }
        v_flex()
            .id("ghost-skills")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(10.0))
            .child(modal_section_title(&p, "Its own skills"))
            .child(own)
            .child(modal_section_title(&p, "Installed skills"))
            .child(modal_hint(&p, "Every agent already sees these. Adding one gives the Ghost its own copy it can change."))
            .child(available)
            .into_any_element()
    }
}

impl Render for GpuiGhostModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let title = if self.is_new {
            "New Ghost".to_string()
        } else {
            let name = self.name_input.read(cx).value().trim().to_string();
            if name.is_empty() { "Ghost".to_string() } else { name }
        };
        let description = if self.is_new {
            "A long-lived agent with a Soul and a Memory that looks after your projects, opens threads in them and runs habits."
        } else {
            "Who it is, what it remembers, what it holds and the skills only it has."
        };
        let mut content = vec![modal_header(&p, title, Some(description))];
        if !self.is_new {
            let selected = TABS.iter().position(|tab| *tab == self.tab).unwrap_or(0);
            content.push(modal_segmented_control(
                &p,
                "ghost-tabs",
                &[
                    ModalSegmentedItem { icon: None, label: "General" },
                    ModalSegmentedItem { icon: None, label: "Soul" },
                    ModalSegmentedItem { icon: None, label: "Memory" },
                    ModalSegmentedItem { icon: None, label: "Skills" },
                ],
                selected,
                |this: &mut Self, index, _, cx| {
                    this.tab = TABS[index];
                    this.error = None;
                    this.notice = None;
                    this.confirming = None;
                    cx.notify();
                },
                cx,
            ));
        }
        content.push(match self.tab {
            Tab::General => self.render_general(window, cx),
            Tab::Soul | Tab::Memory => self.render_file(window, cx),
            Tab::Skills => self.render_skills(cx),
        });
        if let Some(error) = &self.error {
            content.push(modal_error(&p, error.clone()));
        }
        if let Some(notice) = &self.notice {
            content.push(modal_hint(&p, notice.clone()).into_any_element());
        }
        let busy = self.pending.is_some();
        let mut buttons = vec![modal_action_button(
            &p,
            "ghost-modal-cancel",
            if self.is_new { "Cancel" } else { "Close" },
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        )];
        if self.tab != Tab::Skills {
            buttons.push(modal_action_button(
                &p,
                "ghost-modal-save",
                match (busy, self.is_new) {
                    (true, _) => "Saving…",
                    (false, true) => "Create Ghost",
                    (false, false) => "Save",
                },
                None,
                ModalButtonTone::Primary,
                busy,
                |this, window, cx| this.save(window, cx),
                cx,
            ));
        }
        let footer = modal_footer(buttons);
        modal_shell(
            &p,
            "ghostex-gpui-ghost-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiGhostModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}

//! The task panel beside the lanes: Add task (project of the Space, agent, model and effort,
//! optional title, prompt) and a waiting task's editor (title and prompt, then Save, Start or
//! Delete).

use std::cell::Cell;
use std::rc::Rc;

use ghostex_gx_core::session_board::{BoardCard, BoardFilter};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde_json::Value;

use super::picker::PickerTarget;
use super::view::NativeSessionBoardView;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::gx_store::{BoardAgentOption, BoardModelOption};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::{
    KanbanButtonKind, anchor_probe, kanban_button, section_title, text_area, text_field,
};

const PANEL_WIDTH: f32 = 420.0;
const PROMPT_HEIGHT: f32 = 220.0;
/// What the Model and Effort menus call the agent's own choice.
const DEFAULT_CHOICE: &str = "Default";

/// A pick in the panel's agent menu.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct SessionBoardTaskAgent {
    agent_id: String,
}

/// A pick in the panel's model menu; `None` is Default.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct SessionBoardTaskModel {
    model: Option<String>,
}

/// A pick in the panel's effort menu; `None` is Default.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct SessionBoardTaskEffort {
    effort: Option<String>,
}

pub(super) enum PanelMode {
    New,
    /// A waiting task; `loading` until its whole prompt has been read.
    Edit {
        sidebar_session_id: String,
        loading: bool,
    },
}

pub(super) struct TaskPanel {
    mode: PanelMode,
    project_id: Option<String>,
    agent_id: Option<String>,
    agents: Vec<BoardAgentOption>,
    /// The task's own model and effort; `None` leaves the agent's default.
    model: Option<String>,
    effort: Option<String>,
    title: Entity<InputState>,
    prompt: Entity<TextareaState>,
    project_trigger: Rc<Cell<Bounds<Pixels>>>,
    agent_trigger: Rc<Cell<Bounds<Pixels>>>,
    model_trigger: Rc<Cell<Bounds<Pixels>>>,
    effort_trigger: Rc<Cell<Bounds<Pixels>>>,
    saving: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl TaskPanel {
    /// The task this panel edits, if it edits one.
    pub(super) fn editing(&self) -> Option<&str> {
        match &self.mode {
            PanelMode::Edit {
                sidebar_session_id, ..
            } => Some(sidebar_session_id),
            PanelMode::New => None,
        }
    }

    pub(super) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub(super) fn set_project_id(&mut self, project_id: String) {
        self.project_id = Some(project_id);
    }

    fn agent(&self) -> Option<&BoardAgentOption> {
        let agent_id = self.agent_id.as_deref()?;
        self.agents.iter().find(|agent| agent.agent_id == agent_id)
    }

    fn model_option(&self) -> Option<&BoardModelOption> {
        let model = self.model.as_deref()?;
        self.agent()?
            .models
            .iter()
            .find(|option| option.value == model)
    }

    /// The efforts the chosen model accepts, or the agent's own list while the model is Default.
    fn efforts(&self) -> &[(String, String)] {
        match self.model_option() {
            Some(model) => &model.efforts,
            None => self
                .agent()
                .map(|agent| agent.efforts.as_slice())
                .unwrap_or_default(),
        }
    }
}

impl NativeSessionBoardView {
    fn new_panel(
        &mut self,
        mode: PanelMode,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskPanel {
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Optional")
                .default_value(title.to_string())
        });
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("What should the agent do?")
        });
        let subscriptions = vec![
            cx.subscribe_in(&title_input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Focus) {
                    this.input_focused(cx);
                }
                cx.notify();
            }),
            cx.subscribe_in(&prompt, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Focus) {
                    this.input_focused(cx);
                }
                cx.notify();
            }),
        ];
        let (agents, preferred) = self
            .app
            .upgrade()
            .map(|app| app.read(cx).session_board_agent_options())
            .unwrap_or_default();
        TaskPanel {
            mode,
            project_id: None,
            agent_id: preferred,
            agents,
            model: None,
            effort: None,
            title: title_input,
            prompt,
            project_trigger: Rc::new(Cell::new(Bounds::default())),
            agent_trigger: Rc::new(Cell::new(Bounds::default())),
            model_trigger: Rc::new(Cell::new(Bounds::default())),
            effort_trigger: Rc::new(Cell::new(Bounds::default())),
            saving: false,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn open_new_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_chat(window, cx);
        let mut panel = self.new_panel(PanelMode::New, "", window, cx);
        panel.project_id = match &self.board.filter {
            BoardFilter::Project(project_id) => Some(project_id.clone()),
            BoardFilter::All => self
                .board
                .projects
                .first()
                .map(|project| project.project_id.clone()),
        };
        let prompt = panel.prompt.clone();
        self.panel = Some(panel);
        prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
        cx.notify();
    }

    pub(super) fn open_edit_panel(
        &mut self,
        card: &BoardCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_chat(window, cx);
        let mut panel = self.new_panel(
            PanelMode::Edit {
                sidebar_session_id: card.sidebar_session_id.clone(),
                loading: true,
            },
            card.title(),
            window,
            cx,
        );
        panel.project_id = Some(card.project_id.clone());
        let preview = card.task_preview.clone().unwrap_or_default();
        panel
            .prompt
            .update(cx, |prompt, cx| prompt.set_value(preview, window, cx));
        self.panel = Some(panel);
        cx.notify();
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let read = app.update(cx, |app, cx| app.session_board_read_task(card, cx));
        let id = card.sidebar_session_id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(panel) = this.panel.as_mut().filter(|panel| panel.editing() == Some(&id))
                else {
                    return;
                };
                panel.mode = PanelMode::Edit {
                    sidebar_session_id: id.clone(),
                    loading: false,
                };
                match result {
                    Ok(task) => {
                        if let Some(prompt) = task.get("prompt").and_then(Value::as_str) {
                            let prompt = prompt.to_string();
                            panel
                                .prompt
                                .update(cx, |input, cx| input.set_value(prompt, window, cx));
                        }
                    }
                    Err(error) => panel.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.panel = None;
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target() == PickerTarget::TaskProject)
        {
            self.picker = None;
        }
        cx.notify();
    }

    pub(super) fn pick_task_agent(
        &mut self,
        action: &SessionBoardTaskAgent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.panel.as_mut()
            && panel.agent_id.as_deref() != Some(action.agent_id.as_str())
        {
            panel.agent_id = Some(action.agent_id.clone());
            // Another agent has another lineup.
            panel.model = None;
            panel.effort = None;
            cx.notify();
        }
    }

    pub(super) fn pick_task_model(
        &mut self,
        action: &SessionBoardTaskModel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.panel.as_mut() {
            panel.model = action.model.clone();
            // An effort the new model does not accept goes back to Default.
            if let Some(effort) = panel.effort.clone()
                && !panel.efforts().iter().any(|(value, _)| *value == effort)
            {
                panel.effort = None;
            }
            cx.notify();
        }
    }

    pub(super) fn pick_task_effort(
        &mut self,
        action: &SessionBoardTaskEffort,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.panel.as_mut() {
            panel.effort = action.effort.clone();
            cx.notify();
        }
    }

    fn show_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(trigger) = self.panel.as_ref().map(|panel| panel.project_trigger.get()) else {
            return;
        };
        self.toggle_project_picker(PickerTarget::TaskProject, trigger, window, cx);
    }

    fn show_agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let menu = panel
            .agents
            .iter()
            .fold(GpuiContextMenu::new(), |menu, agent| {
                menu.menu_with_check(
                    agent.name.clone(),
                    panel.agent_id.as_deref() == Some(agent.agent_id.as_str()),
                    Box::new(SessionBoardTaskAgent {
                        agent_id: agent.agent_id.clone(),
                    }),
                )
            });
        let trigger = panel.agent_trigger.get();
        self.focus.focus(window, cx);
        menu.toggle_below(trigger, window, cx);
    }

    fn show_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let Some(agent) = panel.agent() else {
            return;
        };
        let menu = GpuiContextMenu::new()
            .menu_with_check(
                DEFAULT_CHOICE,
                panel.model.is_none(),
                Box::new(SessionBoardTaskModel { model: None }),
            )
            .separator();
        let menu = agent.models.iter().fold(menu, |menu, model| {
            menu.menu_with_check(
                model.label.clone(),
                panel.model.as_deref() == Some(model.value.as_str()),
                Box::new(SessionBoardTaskModel {
                    model: Some(model.value.clone()),
                }),
            )
        });
        let trigger = panel.model_trigger.get();
        self.focus.focus(window, cx);
        menu.toggle_below(trigger, window, cx);
    }

    fn show_effort_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let menu = GpuiContextMenu::new()
            .menu_with_check(
                DEFAULT_CHOICE,
                panel.effort.is_none(),
                Box::new(SessionBoardTaskEffort { effort: None }),
            )
            .separator();
        let menu = panel.efforts().iter().fold(menu, |menu, (value, label)| {
            menu.menu_with_check(
                label.clone(),
                panel.effort.as_deref() == Some(value.as_str()),
                Box::new(SessionBoardTaskEffort {
                    effort: Some(value.clone()),
                }),
            )
        });
        let trigger = panel.effort_trigger.get();
        self.focus.focus(window, cx);
        menu.toggle_below(trigger, window, cx);
    }

    /// Add to Pending, Save, or Start (`start`) on the panel.
    fn submit_panel(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        if panel.saving {
            return;
        }
        let title = panel.title.read(cx).value().trim().to_string();
        let prompt = panel.prompt.read(cx).value().trim().to_string();
        if prompt.is_empty() {
            panel.error = Some("Write what the agent should do.".to_string());
            cx.notify();
            return;
        }
        let title = (!title.is_empty()).then_some(title);
        let task = match &panel.mode {
            PanelMode::New => {
                let (Some(project_id), Some(agent_id)) =
                    (panel.project_id.clone(), panel.agent_id.clone())
                else {
                    panel.error = Some("Pick a project and an agent.".to_string());
                    cx.notify();
                    return;
                };
                let (model, effort) = (panel.model.clone(), panel.effort.clone());
                self.app.upgrade().map(|app| {
                    app.update(cx, |app, cx| {
                        app.session_board_create_task(
                            &project_id,
                            &agent_id,
                            model.as_deref(),
                            effort.as_deref(),
                            title.as_deref(),
                            &prompt,
                            cx,
                        )
                    })
                })
            }
            PanelMode::Edit {
                sidebar_session_id, ..
            } => {
                let Some(card) = self.board.card(sidebar_session_id).cloned() else {
                    self.close_panel(cx);
                    return;
                };
                self.app.upgrade().map(|app| {
                    app.update(cx, |app, cx| {
                        app.session_board_save_task(&card, title.as_deref(), &prompt, start, cx)
                    })
                })
            }
        };
        let Some(task) = task else {
            return;
        };
        if let Some(panel) = self.panel.as_mut() {
            panel.saving = true;
            panel.error = None;
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.close_panel(cx),
                Err(error) => {
                    if let Some(panel) = this.panel.as_mut() {
                        panel.saving = false;
                        panel.error = Some(error);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn delete_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self
            .panel
            .as_ref()
            .and_then(TaskPanel::editing)
            .and_then(|id| self.board.card(id).cloned())
        else {
            return;
        };
        self.close_panel(cx);
        self.app_later(window, cx, move |app, _, cx| app.session_board_close_thread(&card, cx));
    }

    fn picker(
        &self,
        id: &'static str,
        label: SharedString,
        trigger: Rc<Cell<Bounds<Pixels>>>,
        p: &KanbanPalette,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = p.control_hover;
        div()
            .id(id)
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.control)
            .text_size(px(13.0))
            .text_color(p.foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(anchor_probe(trigger))
            .child(div().truncate().child(label))
            .child(titlebar_svg_icon("titlebar/chevron-down.svg", 12.0, p.muted))
    }

    fn static_field(label: String, p: &KanbanPalette) -> AnyElement {
        div()
            .flex()
            .items_center()
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .text_size(px(13.0))
            .text_color(p.muted)
            .child(div().truncate().child(label))
            .into_any_element()
    }

    pub(super) fn render_panel(
        &self,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(panel) = self.panel.as_ref() else {
            return div().into_any_element();
        };
        let editing_card = panel
            .editing()
            .and_then(|id| self.board.card(id))
            .cloned();
        let is_new = matches!(panel.mode, PanelMode::New);
        let loading = matches!(panel.mode, PanelMode::Edit { loading: true, .. });
        let heading = match &editing_card {
            Some(card) => card.title().to_string(),
            None => "New task".to_string(),
        };
        let project_label: SharedString = panel
            .project_id
            .as_deref()
            .and_then(|id| self.board.projects.iter().find(|project| project.project_id == id))
            .map(|project| project.label())
            .or_else(|| editing_card.as_ref().map(|card| card.project_label()))
            .unwrap_or_else(|| "Pick a project".to_string())
            .into();
        let agent_label: SharedString = if is_new {
            panel
                .agent()
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| "Pick an agent".to_string())
                .into()
        } else {
            editing_card
                .as_ref()
                .and_then(|card| card.row.menu_facts.agent_name.clone())
                .unwrap_or_default()
                .into()
        };
        let busy = panel.saving || loading;
        let field = |label: &'static str, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .min_w_0()
                .child(section_title(label, p))
                .child(body)
        };
        let project_body = if is_new {
            self.picker(
                "session-board-task-project",
                project_label,
                panel.project_trigger.clone(),
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| this.show_project_picker(window, cx)))
            .into_any_element()
        } else {
            Self::static_field(project_label.to_string(), p)
        };
        let agent_body = if is_new {
            self.picker(
                "session-board-task-agent",
                agent_label,
                panel.agent_trigger.clone(),
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| this.show_agent_menu(window, cx)))
            .into_any_element()
        } else {
            Self::static_field(agent_label.to_string(), p)
        };
        // Model and effort are a launch choice, so only a new task offers them, and only on an
        // agent whose launch takes one.
        let model_row = (is_new && panel.agent().is_some_and(|agent| !agent.models.is_empty()))
            .then(|| {
                let model_label: SharedString = panel
                    .model_option()
                    .map(|model| model.label.clone())
                    .unwrap_or_else(|| DEFAULT_CHOICE.to_string())
                    .into();
                let effort_label: SharedString = panel
                    .effort
                    .as_deref()
                    .and_then(|effort| {
                        panel
                            .efforts()
                            .iter()
                            .find(|(value, _)| value == effort)
                            .map(|(_, label)| label.clone())
                    })
                    .unwrap_or_else(|| DEFAULT_CHOICE.to_string())
                    .into();
                let has_efforts = !panel.efforts().is_empty();
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        field(
                            "Model",
                            self.picker(
                                "session-board-task-model",
                                model_label,
                                panel.model_trigger.clone(),
                                p,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_model_menu(window, cx)),
                            )
                            .into_any_element(),
                        )
                        .flex_1(),
                    )
                    .when(has_efforts, |this| {
                        this.child(
                            field(
                                "Effort",
                                self.picker(
                                    "session-board-task-effort",
                                    effort_label,
                                    panel.effort_trigger.clone(),
                                    p,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_effort_menu(window, cx)
                                }))
                                .into_any_element(),
                            )
                            .w(px(130.0))
                            .flex_none(),
                        )
                    })
                    .into_any_element()
            });
        div()
            .id("session-board-task-panel")
            .w(px(PANEL_WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .bg(p.panel)
            .border_l_1()
            .border_color(p.border_strong)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(heading),
                    )
                    .child(
                        kanban_button(
                            "session-board-task-close",
                            Some("titlebar/x.svg"),
                            None,
                            KanbanButtonKind::Ghost,
                            false,
                            p,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.close_panel(cx))),
                    ),
            )
            .child(field("Project", project_body))
            .child(field("Agent", agent_body))
            .children(model_row)
            .child(field("Title", text_field(&panel.title, p, window, cx)))
            .child(field(
                "Prompt",
                text_area(&panel.prompt, PROMPT_HEIGHT, p, window, cx),
            ))
            .when_some(panel.error.clone(), |this, error| {
                this.child(div().text_size(px(12.0)).text_color(p.danger).child(error))
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(!is_new, |this| {
                        this.child(
                            kanban_button(
                                "session-board-task-delete",
                                None,
                                Some("Delete".into()),
                                KanbanButtonKind::Danger,
                                busy,
                                p,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delete_task(window, cx)
                            })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        kanban_button(
                            "session-board-task-cancel",
                            None,
                            Some(if is_new { "Cancel" } else { "Save" }.into()),
                            KanbanButtonKind::Secondary,
                            busy,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if is_new {
                                this.close_panel(cx);
                            } else {
                                this.submit_panel(false, window, cx);
                            }
                        })),
                    )
                    .child(
                        kanban_button(
                            "session-board-task-submit",
                            (!is_new).then_some("titlebar/player-play.svg"),
                            Some(if is_new { "Add to Pending" } else { "Start" }.into()),
                            KanbanButtonKind::Primary,
                            busy,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.submit_panel(!is_new, window, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
}

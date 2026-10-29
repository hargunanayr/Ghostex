//! A Ghost's Overview, beside its chat: a header with the Ghost and its settings, then three tabs.
//! Threads lists the sessions it started in its projects (open one to read it); Documents lists the
//! pages and documents it keeps (an HTML page opens in the page window, a Markdown document reads
//! here); Habits lists its routines, what each runs and what its last runs came to, with Run now,
//! Turn off and a form for a new one.
//!
//! CDXC:Ghosts 2026-10-29 WHY:
//! The page reads three RPCs (`/api/readCoordinator` for the threads, because a Ghost's chat is a coordinator; `/api/ghostArtifacts`; `/api/readAutomationState` of the home project, where every automation is a habit) and holds no rules of its own: what a run came to and how a schedule reads are `ghostex_gx_core::ghosts`, so another client words them the same.

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::gpui_open_external_http_url;
use crate::app::native_automate::{
    AutomatePalette, CARD_RADIUS, ICON_ALERT, ICON_PLAY, ICON_PLUS, ICON_REFRESH, ICON_TRASH,
    ROW_RADIUS, detail_row, empty_state, group_card, icon, icon_button, secondary_button,
    section_label, status_dot, switch,
};
use crate::app::native_sidebar::ghosts::ghost_avatar;
use ghostex_gx_core::ghosts::{
    ghost_thread_state_label, habit_run_active, habit_run_outcome_label, habit_run_posted,
    habit_schedule_label, parse_habit_schedule,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, AppContext as _, Context, ElementId, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Transformation, WeakEntity,
    Window, div, px, radians, rgb,
};
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::text::TextView;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How often the showing Overview reads the Ghost again, for thread states and habit runs.
const REFRESH_INTERVAL: Duration = Duration::from_secs(20);
/// While a habit run is going the Overview reads again this often, so Running turns into what the
/// run came to within seconds instead of up to a full refresh later.
const ACTIVE_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
/// A habit's instructions longer than this start folded to a few lines, so its runs stay in view.
const LONG_INSTRUCTIONS: usize = 360;
const ICON_SETTINGS: &str = "titlebar/settings.svg";
const ICON_LOADER: &str = "titlebar/loader2.svg";
const ICON_EXTERNAL: &str = "titlebar/arrow-up-right.svg";
const ICON_BACK: &str = "titlebar/chevron-left.svg";
const ICON_FILE: &str = "titlebar/file-text.svg";
const ICON_PAGE: &str = "titlebar/world.svg";
const ICON_CLOCK: &str = "titlebar/clock.svg";
const ICON_THREADS: &str = "titlebar/messages.svg";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Threads,
    Documents,
    Habits,
}

enum Drill {
    /// A Markdown document being read; `text` arrives after the open.
    Document {
        entry: Value,
        text: Option<String>,
    },
    Habit(String),
}

struct HabitForm {
    name: Entity<InputState>,
    schedule: Entity<InputState>,
    instructions: Entity<TextareaState>,
    error: Option<String>,
    saving: bool,
}

pub(crate) struct NativeGhostOverviewView {
    app: WeakEntity<GhostexGpuiApp>,
    ghost_id: Option<String>,
    glass: bool,
    tab: Tab,
    ghost: Option<Value>,
    threads: Vec<Value>,
    artifacts: Vec<Value>,
    habits: Vec<Value>,
    runs: Vec<Value>,
    drill: Option<Drill>,
    habit_form: Option<HabitForm>,
    /// The habit whose Run now is on its way to the server, shown as starting until the run exists.
    run_starting: Option<String>,
    /// The open habit's Delete asks once before it deletes.
    confirm_delete: bool,
    /// The open habit's long instructions are shown in full.
    instructions_open: bool,
    error: Option<String>,
    loaded: bool,
    load_generation: u64,
    last_load: Instant,
    _refresh: Task<()>,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// "Today 17:00", "Yesterday 09:12", "Tomorrow 09:00" or "3 Nov 17:00" in this computer's time.
fn day_time(value: &str) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(value) else {
        return String::new();
    };
    let time = time.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let day = time.date_naive();
    let clock = time.format("%H:%M");
    if day == today {
        format!("Today {clock}")
    } else if today.pred_opt() == Some(day) {
        format!("Yesterday {clock}")
    } else if today.succ_opt() == Some(day) {
        format!("Tomorrow {clock}")
    } else {
        time.format("%-d %b %H:%M").to_string()
    }
}

/// How long ago a run started: "just now", "4 min ago", then the clock time.
fn started_ago(value: &str) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(value) else {
        return "just now".to_string();
    };
    let minutes = (chrono::Utc::now() - time.with_timezone(&chrono::Utc)).num_minutes();
    match minutes {
        ..=0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        _ => format!("at {}", time.with_timezone(&chrono::Local).format("%H:%M")),
    }
}

/// The rotating loader that marks something in progress (a running habit, a working thread).
fn spinner(id: impl Into<ElementId>, size: f32, color: Hsla) -> AnyElement {
    icon(ICON_LOADER, size, color)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

impl NativeGhostOverviewView {
    pub(crate) fn new(
        app: WeakEntity<GhostexGpuiApp>,
        ghost_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(ACTIVE_REFRESH_INTERVAL)
                    .await;
                let showing = this
                    .update(cx, |view, cx| {
                        let showing = view.app.upgrade().is_some_and(|app| {
                            app.read(cx).active_mode
                                == crate::app::model::TitlebarMode::GhostOverview
                        });
                        if showing
                            && (view.any_run_active()
                                || view.last_load.elapsed() >= REFRESH_INTERVAL)
                        {
                            view.load(cx);
                        }
                    })
                    .is_ok();
                if !showing {
                    break;
                }
            }
        });
        Self {
            app,
            ghost_id,
            glass: false,
            tab: Tab::Threads,
            ghost: None,
            threads: Vec::new(),
            artifacts: Vec::new(),
            habits: Vec::new(),
            runs: Vec::new(),
            drill: None,
            habit_form: None,
            run_starting: None,
            confirm_delete: false,
            instructions_open: false,
            error: None,
            loaded: false,
            load_generation: 0,
            last_load: Instant::now(),
            _refresh: refresh,
        }
    }

    /// Opens a document or habit (or goes back to the list), forgetting the previous one's
    /// unanswered Delete and unfolded instructions.
    fn set_drill(&mut self, drill: Option<Drill>, cx: &mut Context<Self>) {
        self.drill = drill;
        self.confirm_delete = false;
        self.instructions_open = false;
        cx.notify();
    }

    /// Whether the window glass changed since the last frame, which redraws the cached page.
    pub(crate) fn sync(&mut self, glass: bool) -> bool {
        std::mem::replace(&mut self.glass, glass) != glass
    }

    /// Switches to another Ghost, dropping what was open for the previous one.
    pub(crate) fn show_ghost(&mut self, ghost_id: String, cx: &mut Context<Self>) {
        if self.ghost_id.as_deref() == Some(ghost_id.as_str()) {
            return;
        }
        self.ghost_id = Some(ghost_id);
        self.ghost = None;
        self.threads.clear();
        self.artifacts.clear();
        self.habits.clear();
        self.runs.clear();
        self.drill = None;
        self.habit_form = None;
        self.run_starting = None;
        self.confirm_delete = false;
        self.instructions_open = false;
        self.loaded = false;
        self.load(cx);
    }

    /// Reads the Ghost, its threads, its documents and its habits; the page keeps what it shows
    /// until the answers arrive.
    pub(crate) fn load(&mut self, cx: &mut Context<Self>) {
        let Some(ghost_id) = self.ghost_id.clone() else {
            return;
        };
        self.load_generation += 1;
        self.last_load = Instant::now();
        let generation = self.load_generation;
        cx.spawn(async move |this, cx| {
            let ghost = gx_rpc(None, "/api/readGhost", json!({ "ghostId": ghost_id }))
                .await
                .map(|value| value["ghost"].clone());
            let (threads, artifacts, automation) = match &ghost {
                Ok(ghost) => {
                    let chat = &ghost["chatSession"];
                    let threads = gx_rpc(
                        None,
                        "/api/readCoordinator",
                        json!({ "projectId": chat["projectId"], "sessionId": chat["sessionId"] }),
                    )
                    .await
                    .ok();
                    let artifacts = gx_rpc(
                        None,
                        "/api/ghostArtifacts",
                        json!({ "ghostId": ghost_id, "action": "list" }),
                    )
                    .await
                    .ok();
                    let automation = gx_rpc(
                        None,
                        "/api/readAutomationState",
                        json!({ "projectId": ghost["homeProjectId"] }),
                    )
                    .await
                    .ok();
                    (threads, artifacts, automation)
                }
                Err(_) => (None, None, None),
            };
            let _ = this.update(cx, |view, cx| {
                if view.load_generation != generation {
                    return;
                }
                view.loaded = true;
                match ghost {
                    Ok(ghost) => {
                        view.error = None;
                        view.ghost = Some(ghost);
                    }
                    Err(error) => view.error = Some(error.to_string()),
                }
                let list = |value: Option<&Value>, pointer: &str| {
                    value
                        .and_then(|value| value.pointer(pointer))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                };
                view.threads = list(threads.as_ref(), "/threads");
                view.artifacts = list(artifacts.as_ref(), "/artifacts");
                view.habits = list(automation.as_ref(), "/automationState/automations");
                view.runs = list(automation.as_ref(), "/automationState/runs");
                cx.notify();
            });
        })
        .detach();
    }

    fn with_app(
        &self,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut GhostexGpuiApp, &mut Context<GhostexGpuiApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, action);
        }
    }

    fn home_project_id(&self) -> Value {
        self.ghost
            .as_ref()
            .map(|ghost| ghost["homeProjectId"].clone())
            .unwrap_or(Value::Null)
    }

    /// Runs one Automate call on a habit, shows the habits and runs it answers with straight away,
    /// then reads the page again.
    fn habit_call(&mut self, path: &'static str, params: Value, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, path, params).await;
            let _ = this.update(cx, |view, cx| {
                view.run_starting = None;
                match result {
                    Ok(value) => {
                        let state = &value["automationState"];
                        if let Some(habits) = state["automations"].as_array() {
                            view.habits = habits.clone();
                        }
                        if let Some(runs) = state["runs"].as_array() {
                            view.runs = runs.clone();
                        }
                    }
                    Err(error) => view.error = Some(error.to_string()),
                }
                cx.notify();
                view.load(cx);
            });
        })
        .detach();
    }

    /// The habit's run that is still going, if any.
    fn active_run(&self, habit_id: &str) -> Option<&Value> {
        self.habit_runs(habit_id)
            .into_iter()
            .find(|run| habit_run_active(&text(run, "status")))
    }

    fn any_run_active(&self) -> bool {
        self.run_starting.is_some()
            || self
                .runs
                .iter()
                .any(|run| habit_run_active(&text(run, "status")))
    }

    /// Run now: the habit shows as starting at once and as running once the server has the run.
    /// A habit that is already running is left alone, since the server would only skip the run.
    fn run_habit(&mut self, habit_id: String, cx: &mut Context<Self>) {
        if self.run_starting.is_some() || self.active_run(&habit_id).is_some() {
            return;
        }
        self.run_starting = Some(habit_id.clone());
        self.error = None;
        cx.notify();
        self.habit_call(
            "/api/runAutomationNow",
            json!({ "projectId": self.home_project_id(), "automationId": habit_id }),
            cx,
        );
    }

    /// Opens the session a habit run works in, where its agent can be watched.
    fn open_run_session(&mut self, run: &Value, cx: &mut Context<Self>) {
        let session = text(run, "sessionId");
        let (project_id, session_id) = match session.split_once(':') {
            Some((project_id, session_id)) => (project_id.to_string(), session_id.to_string()),
            None => (text(run, "projectId"), session),
        };
        if session_id.is_empty() {
            return;
        }
        self.with_app(cx, |app, cx| {
            app.focus_session_from_ghost_overview(&project_id, &session_id, cx)
        });
    }

    fn open_document(&mut self, entry: Value, cx: &mut Context<Self>) {
        if text(&entry, "kind") == "html" {
            let url = text(&entry, "url");
            if url.starts_with("http") {
                let message = json!({ "url": url, "title": text(&entry, "title"), "file": "" });
                self.with_app(cx, |app, cx| app.open_gpui_visual_page_modal(&message, cx));
            } else if !url.is_empty() {
                let _ = gpui_open_external_http_url(&url);
            }
            return;
        }
        let ghost_id = self.ghost_id.clone().unwrap_or_default();
        let id = text(&entry, "id");
        self.drill = Some(Drill::Document { entry, text: None });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = gx_rpc(
                None,
                "/api/ghostArtifacts",
                json!({ "ghostId": ghost_id, "action": "read", "id": id }),
            )
            .await;
            let _ = this.update(cx, |view, cx| {
                if let Some(Drill::Document { text: body, .. }) = &mut view.drill {
                    *body = Some(match read {
                        Ok(value) => text(&value["artifact"], "text"),
                        Err(error) => format!("Could not read the document: {error}"),
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn start_habit_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Daily recap"));
        let schedule = cx.new(|cx| InputState::new(window, cx).placeholder("daily 17:20"));
        let instructions = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Read today's commits in my projects and tell me what is still open.")
        });
        name.update(cx, |input, cx| input.focus(window, cx));
        self.habit_form = Some(HabitForm {
            name,
            schedule,
            instructions,
            error: None,
            saving: false,
        });
        cx.notify();
    }

    fn save_habit_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.habit_form else {
            return;
        };
        let name = form.name.read(cx).value().trim().to_string();
        let instructions = form.instructions.read(cx).value().trim().to_string();
        let schedule = parse_habit_schedule(form.schedule.read(cx).value().as_ref());
        let schedule = match (name.is_empty(), instructions.is_empty(), schedule) {
            (true, _, _) => Err("Give the habit a name.".to_string()),
            (_, true, _) => Err("Write what the habit does.".to_string()),
            (_, _, schedule) => schedule,
        };
        let schedule = match schedule {
            Ok(schedule) => schedule,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return;
            }
        };
        form.saving = true;
        form.error = None;
        let agent_id = self
            .ghost
            .as_ref()
            .map(|ghost| ghost["chatSession"]["agentId"].clone())
            .unwrap_or(Value::Null);
        let params = json!({
            "projectId": self.home_project_id(),
            "definition": {
                "name": name,
                "agentId": agent_id,
                "prompt": instructions,
                "enabled": true,
                "schedule": schedule,
                "executionMode": { "kind": "local" },
            },
        });
        cx.spawn(async move |this, cx| {
            let saved = gx_rpc(None, "/api/saveAutomation", params).await;
            let _ = this.update(cx, |view, cx| {
                match saved {
                    Ok(_) => view.habit_form = None,
                    Err(error) => {
                        if let Some(form) = &mut view.habit_form {
                            form.saving = false;
                            form.error = Some(error.to_string());
                        }
                    }
                }
                view.load(cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn render_header(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        let ghost = self.ghost.as_ref();
        let name = ghost.map(|ghost| text(ghost, "name")).unwrap_or_default();
        let ghost_id = self.ghost_id.clone().unwrap_or_default();
        let projects = ghost
            .and_then(|ghost| ghost["projects"].as_array())
            .map(|projects| {
                projects
                    .iter()
                    .map(|project| text(project, "name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let settings_id = ghost_id.clone();
        h_flex()
            .w_full()
            .gap(px(12.0))
            .items_center()
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(10.0))
            .child(ghost_avatar(
                &ghost_id,
                &name,
                &ghost.map(|ghost| text(ghost, "mascot")).unwrap_or_default(),
                &ghost.map(|ghost| text(ghost, "color")).unwrap_or_default(),
                34.0,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .truncate()
                            .child(if name.is_empty() {
                                "Ghost".to_string()
                            } else {
                                name
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.muted)
                            .truncate()
                            .child(if projects.is_empty() {
                                "No projects yet. Add some in Ghost settings.".to_string()
                            } else {
                                format!("Looks after {projects}")
                            }),
                    ),
            )
            .child(icon_button(
                p,
                "ghost-overview-refresh",
                ICON_REFRESH,
                "Refresh",
                false,
                |view: &mut Self, _, cx| view.load(cx),
                cx,
            ))
            .child(icon_button(
                p,
                "ghost-overview-settings",
                ICON_SETTINGS,
                "Ghost settings",
                false,
                move |view: &mut Self, window, cx| {
                    let ghost_id = settings_id.clone();
                    if let Some(app) = view.app.upgrade() {
                        app.update(cx, |app, cx| {
                            app.open_ghost_settings_modal(&ghost_id, window, cx)
                        });
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    fn render_tabs(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        let running = self.any_run_active();
        let tab = |label: &'static str,
                   count: usize,
                   value: Tab,
                   id: &'static str,
                   cx: &mut Context<Self>| {
            let selected = self.tab == value;
            h_flex()
                .id(id)
                .role(gpui::Role::Tab)
                .aria_selected(selected)
                .gap(px(6.0))
                .items_center()
                .h(px(30.0))
                .px(px(12.0))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .text_size(px(13.0))
                .text_color(if selected { p.foreground } else { p.muted })
                .when(selected, |this| this.bg(p.selected))
                .when(!selected, |this| this.hover(|this| this.bg(p.hover)))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.tab = value;
                    view.habit_form = None;
                    view.set_drill(None, cx);
                }))
                .child(label)
                .when(value == Tab::Habits && running, |this| {
                    this.child(spinner("ghost-tab-habits-running", 12.0, p.accent))
                })
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.muted.opacity(0.8))
                            .child(count.to_string()),
                    )
                })
        };
        let open_threads = self
            .threads
            .iter()
            .filter(|thread| text(thread, "state") != "done")
            .count();
        h_flex()
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(p.border)
            .child(tab(
                "Threads",
                open_threads,
                Tab::Threads,
                "ghost-tab-threads",
                cx,
            ))
            .child(tab(
                "Documents",
                self.artifacts.len(),
                Tab::Documents,
                "ghost-tab-documents",
                cx,
            ))
            .child(tab(
                "Habits",
                self.habits.len(),
                Tab::Habits,
                "ghost-tab-habits",
                cx,
            ))
            .into_any_element()
    }

    fn row(p: &AutomatePalette, id: SharedString) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .w_full()
            .gap(px(10.0))
            .items_center()
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(|this| this.bg(p.hover))
    }

    fn two_lines(p: &AutomatePalette, title: String, detail: String) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(13.5))
                    .text_color(p.foreground)
                    .truncate()
                    .child(title),
            )
            .when(!detail.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .truncate()
                        .child(detail),
                )
            })
            .into_any_element()
    }

    fn render_threads(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        if self.threads.is_empty() {
            return empty_state(
                p,
                ICON_THREADS,
                "No threads yet",
                "Ask the Ghost for real work in one of its projects and it starts a thread here.",
                None,
            );
        }
        let mut list = v_flex().w_full().gap(px(2.0)).px(px(8.0)).pt(px(8.0));
        for (index, thread) in self.threads.iter().enumerate() {
            let state = text(thread, "state");
            let color: Hsla = match state.as_str() {
                "waiting" => rgb(0xf472b6),
                "working" => rgb(0xc68a06),
                "finished" => rgb(0x95d7f6),
                _ => rgb(0x8a8f98),
            }
            .into();
            let marker = if state == "working" {
                spinner(
                    SharedString::from(format!("ghost-thread-working-{index}")),
                    14.0,
                    color,
                )
            } else {
                div()
                    .size(px(14.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(status_dot(color).size(px(8.0)))
                    .into_any_element()
            };
            let project_id = text(thread, "projectId");
            let session_id = text(thread, "sessionId");
            let project = self
                .ghost
                .as_ref()
                .and_then(|ghost| ghost["projects"].as_array())
                .and_then(|projects| {
                    projects
                        .iter()
                        .find(|project| text(project, "projectId") == project_id)
                })
                .map(|project| text(project, "name"))
                .unwrap_or_default();
            let title = Some(text(thread, "title"))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| text(thread, "task"));
            list = list.child(
                Self::row(p, SharedString::from(format!("ghost-thread-{index}")))
                    .child(marker)
                    .child(Self::two_lines(
                        p,
                        title,
                        if project.is_empty() {
                            ghost_thread_state_label(&state).to_string()
                        } else {
                            format!("{} · {project}", ghost_thread_state_label(&state))
                        },
                    ))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let (project_id, session_id) = (project_id.clone(), session_id.clone());
                        view.with_app(cx, |app, cx| {
                            app.focus_session_from_ghost_overview(&project_id, &session_id, cx)
                        });
                    })),
            );
        }
        list.into_any_element()
    }

    fn render_documents(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        if let Some(Drill::Document { entry, text: body }) = &self.drill {
            return v_flex()
                .w_full()
                .px(px(16.0))
                .pt(px(12.0))
                .gap(px(12.0))
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .ml(px(-6.0))
                        .child(icon_button(
                            p,
                            "ghost-document-back",
                            ICON_BACK,
                            "Back to documents",
                            false,
                            |view: &mut Self, _, cx| view.set_drill(None, cx),
                            cx,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(15.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.foreground)
                                .child(text(entry, "title")),
                        ),
                )
                .child(match body {
                    Some(body) => TextView::markdown(
                        SharedString::from(format!("ghost-document-{}", text(entry, "id"))),
                        body.clone(),
                    )
                    .min_w_0()
                    .on_link_click(|href, event, _, _| {
                        if event.standard_click() {
                            let _ = gpui_open_external_http_url(href);
                        }
                    })
                    .into_any_element(),
                    None => h_flex()
                        .gap(px(8.0))
                        .text_size(px(13.0))
                        .text_color(p.muted)
                        .child(spinner("ghost-document-reading", 14.0, p.muted))
                        .child("Reading…")
                        .into_any_element(),
                })
                .into_any_element();
        }
        if self.artifacts.is_empty() {
            return empty_state(
                p,
                ICON_FILE,
                "No documents yet",
                "Reports, dashboards and plans the Ghost writes for you are kept here.",
                None,
            );
        }
        let mut list = v_flex().w_full().gap(px(2.0)).px(px(8.0)).pt(px(8.0));
        for (index, entry) in self.artifacts.iter().enumerate() {
            let html = text(entry, "kind") == "html";
            let summary = text(entry, "summary");
            let detail = if summary.is_empty() {
                format!(
                    "{} · {}",
                    if html { "Visual" } else { "Document" },
                    day_time(&text(entry, "updatedAt"))
                )
            } else {
                summary
            };
            let opened = entry.clone();
            list = list.child(
                Self::row(p, SharedString::from(format!("ghost-document-{index}")))
                    .child(icon(
                        if html { ICON_PAGE } else { ICON_FILE },
                        16.0,
                        p.muted,
                    ))
                    .child(Self::two_lines(p, text(entry, "title"), detail))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.open_document(opened.clone(), cx)),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The runs of one habit, newest first.
    fn habit_runs(&self, habit_id: &str) -> Vec<&Value> {
        let mut runs = self
            .runs
            .iter()
            .filter(|run| text(run, "automationId") == habit_id)
            .collect::<Vec<_>>();
        runs.sort_by_key(|run| std::cmp::Reverse(text(run, "createdAt")));
        runs
    }

    fn render_habit_detail(
        &self,
        p: &AutomatePalette,
        habit: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = text(habit, "id");
        let enabled = habit["enabled"].as_bool() == Some(true);
        let project_id = self.home_project_id();
        let active = self.active_run(&id).cloned();
        let starting = self.run_starting.as_deref() == Some(id.as_str());
        let value = |value: String| div().child(value).into_any_element();

        // Run now turns into a Running pill for as long as a run is going, so the click always
        // shows that it took and a second click cannot start a run the server would skip.
        let run_control = if starting || active.is_some() {
            h_flex()
                .id("ghost-habit-running")
                .role(gpui::Role::Status)
                .aria_label("Habit running")
                .flex_shrink_0()
                .h(px(32.0))
                .px(px(12.0))
                .gap(px(6.0))
                .items_center()
                .rounded(px(ROW_RADIUS))
                .bg(p.accent.opacity(0.12))
                .text_size(px(13.0))
                .text_color(p.accent)
                .child(spinner("ghost-habit-run-spinner", 14.0, p.accent))
                .child(if active.is_some() {
                    "Running"
                } else {
                    "Starting…"
                })
                .into_any_element()
        } else {
            let run_id = id.clone();
            secondary_button(
                p,
                "ghost-habit-run",
                Some(ICON_PLAY),
                "Run now",
                move |view: &mut Self, _, cx| view.run_habit(run_id.clone(), cx),
                cx,
            )
            .into_any_element()
        };
        let header = h_flex()
            .gap(px(6.0))
            .ml(px(-6.0))
            .child(icon_button(
                p,
                "ghost-habit-back",
                ICON_BACK,
                "Back to habits",
                false,
                |view: &mut Self, _, cx| view.set_drill(None, cx),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(text(habit, "name")),
            )
            .child(run_control)
            .child(icon_button(
                p,
                "ghost-habit-delete",
                ICON_TRASH,
                "Delete habit",
                self.confirm_delete,
                |view: &mut Self, _, cx| {
                    view.confirm_delete = true;
                    cx.notify();
                },
                cx,
            ));

        let confirm = self.confirm_delete.then(|| {
            let (delete_id, delete_project) = (id.clone(), project_id.clone());
            h_flex()
                .w_full()
                .gap(px(8.0))
                .items_center()
                .p(px(12.0))
                .rounded(px(CARD_RADIUS))
                .border_1()
                .border_color(p.danger.opacity(0.35))
                .bg(p.danger.opacity(0.08))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.0))
                        .text_color(p.foreground)
                        .child("Delete this habit and its run history? This can't be undone."),
                )
                .child(secondary_button(
                    p,
                    "ghost-habit-delete-cancel",
                    None,
                    "Cancel",
                    |view: &mut Self, _, cx| {
                        view.confirm_delete = false;
                        cx.notify();
                    },
                    cx,
                ))
                .child(
                    secondary_button(
                        p,
                        "ghost-habit-delete-confirm",
                        None,
                        "Delete",
                        move |view: &mut Self, _, cx| {
                            view.set_drill(None, cx);
                            view.habit_call(
                                "/api/deleteAutomation",
                                json!({ "projectId": delete_project, "automationId": delete_id }),
                                cx,
                            )
                        },
                        cx,
                    )
                    .text_color(p.danger),
                )
        });

        let running_card = active.as_ref().map(|run| {
            let has_session = !text(run, "sessionId").is_empty();
            let opened = run.clone();
            h_flex()
                .w_full()
                .gap(px(12.0))
                .items_center()
                .px(px(16.0))
                .py(px(12.0))
                .rounded(px(CARD_RADIUS))
                .border_1()
                .border_color(p.accent.opacity(0.35))
                .bg(p.accent.opacity(0.08))
                .child(spinner("ghost-habit-running-card", 18.0, p.accent))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(p.foreground)
                                .child("Running now"),
                        )
                        .child(div().text_size(px(12.5)).text_color(p.muted).child(format!(
                            "Started {}. The Ghost messages you if there is something to tell.",
                            started_ago(&text(run, "createdAt"))
                        ))),
                )
                .when(has_session, |this| {
                    this.child(secondary_button(
                        p,
                        "ghost-habit-running-open",
                        Some(ICON_EXTERNAL),
                        "Watch",
                        move |view: &mut Self, _, cx| view.open_run_session(&opened, cx),
                        cx,
                    ))
                })
        });

        let (toggle_id, toggle_project) = (id.clone(), project_id);
        let toggle = div()
            .id("ghost-habit-toggle")
            .role(gpui::Role::Switch)
            .aria_label("Runs on schedule")
            .aria_toggled(if enabled {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .cursor_pointer()
            .on_click(cx.listener(move |view, _, _, cx| {
                // The switch moves at once; the answer from the server settles it.
                if let Some(habit) = view
                    .habits
                    .iter_mut()
                    .find(|habit| text(habit, "id") == toggle_id)
                {
                    habit["enabled"] = Value::Bool(!enabled);
                }
                cx.notify();
                view.habit_call(
                    "/api/setAutomationEnabled",
                    json!({
                        "projectId": toggle_project,
                        "automationId": toggle_id,
                        "enabled": !enabled,
                    }),
                    cx,
                )
            }))
            .child(switch(p, enabled, false))
            .into_any_element();
        let settings = group_card(
            p,
            vec![
                detail_row(
                    p,
                    "Schedule",
                    vec![value(habit_schedule_label(&habit["schedule"]))],
                ),
                detail_row(
                    p,
                    "Next run",
                    vec![value(if enabled {
                        day_time(&text(habit, "nextRunAt"))
                    } else {
                        "Off".to_string()
                    })],
                ),
                detail_row(p, "Runs on schedule", vec![toggle]),
            ],
        );

        let habit_runs = self.habit_runs(&id);
        let runs = if habit_runs.is_empty() {
            div()
                .px(px(2.0))
                .text_size(px(13.0))
                .text_color(p.muted)
                .child("No runs yet. Run now tries it once without waiting for the schedule.")
                .into_any_element()
        } else {
            let rows = habit_runs
                .into_iter()
                .take(10)
                .enumerate()
                .map(|(index, run)| {
                    let status = text(run, "status");
                    let is_active = habit_run_active(&status);
                    let color = if habit_run_posted(&status) {
                        p.success
                    } else {
                        p.run_status_color(&status)
                    };
                    let has_session = !text(run, "sessionId").is_empty();
                    let opened = run.clone();
                    h_flex()
                        .id(SharedString::from(format!("ghost-habit-run-{index}")))
                        .w_full()
                        .gap(px(10.0))
                        .items_center()
                        .px(px(16.0))
                        .py(px(10.0))
                        .child(if is_active {
                            spinner(
                                SharedString::from(format!("ghost-habit-run-spin-{index}")),
                                14.0,
                                p.accent,
                            )
                        } else {
                            div()
                                .size(px(14.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(status_dot(color).size(px(7.0)))
                                .into_any_element()
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.0))
                                .text_color(if is_active { p.accent } else { p.foreground })
                                .child(habit_run_outcome_label(&status)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .child(day_time(&text(run, "createdAt"))),
                        )
                        .when(has_session, |this| {
                            this.cursor_pointer()
                                .hover(|this| this.bg(p.hover))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.open_run_session(&opened, cx)
                                }))
                        })
                        .into_any_element()
                })
                .collect();
            group_card(p, rows).into_any_element()
        };

        let instructions = text(habit, "prompt");
        let long = instructions.chars().count() > LONG_INSTRUCTIONS;
        let folded = long && !self.instructions_open;
        let instructions_card = v_flex()
            .w_full()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(p.border)
            .bg(p.card)
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(p.foreground.opacity(0.9))
                    .when(folded, |this| this.line_clamp(5))
                    .child(instructions),
            )
            .when(long, |this| {
                this.child(
                    div()
                        .id("ghost-habit-instructions-toggle")
                        .role(gpui::Role::Button)
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .text_color(p.muted)
                        .hover(|this| this.text_color(p.foreground))
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.instructions_open = !view.instructions_open;
                            cx.notify();
                        }))
                        .child(if folded { "Show all" } else { "Show less" }),
                )
            });

        let section = |label: &'static str, body: AnyElement| {
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(section_label(p, label).px(px(2.0)))
                .child(body)
        };
        v_flex()
            .w_full()
            .px(px(16.0))
            .pt(px(12.0))
            .gap(px(16.0))
            .child(header)
            .children(confirm)
            .children(running_card)
            .child(settings)
            .child(section("Recent runs", runs))
            .child(section(
                "What it does",
                instructions_card.into_any_element(),
            ))
            .into_any_element()
    }

    fn render_habit_form(
        &self,
        p: &AutomatePalette,
        form: &HabitForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = |label: &'static str, hint: Option<&'static str>, control: AnyElement| {
            v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.foreground)
                        .child(label),
                )
                .child(control)
                .when_some(hint, |this, hint| {
                    this.child(div().text_size(px(12.0)).text_color(p.muted).child(hint))
                })
        };
        let saving = form.saving;
        let (save_fill, save_text) = (p.control_primary, p.switch_thumb);
        let save = h_flex()
            .id("ghost-habit-form-save")
            .role(gpui::Role::Button)
            .flex_shrink_0()
            .h(px(32.0))
            .px(px(14.0))
            .gap(px(6.0))
            .items_center()
            .rounded(px(ROW_RADIUS))
            .bg(save_fill)
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(save_text)
            .when(saving, |this| this.opacity(0.7))
            .when(!saving, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(save_fill.opacity(0.88)))
                    .on_click(cx.listener(|view, _, _, cx| view.save_habit_form(cx)))
            })
            .child(if saving {
                spinner("ghost-habit-form-saving", 14.0, save_text)
            } else {
                icon(ICON_PLUS, 15.0, save_text).into_any_element()
            })
            .child(if saving { "Adding…" } else { "Add habit" });
        v_flex()
            .w_full()
            .px(px(16.0))
            .pt(px(14.0))
            .gap(px(14.0))
            .child(
                v_flex()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child("New habit"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.muted)
                            .child("The Ghost runs it on its own and messages you when there is something to tell."),
                    ),
            )
            .child(field("Name", None, Input::new(&form.name).into_any_element()))
            .child(field(
                "When",
                Some("hourly :00 · daily 17:20 · weekdays 09:00 · weekly mon 09:00, in this computer's time"),
                Input::new(&form.schedule).into_any_element(),
            ))
            .child(field(
                "What it does",
                Some("Say what to look at, what to tell you, and when to stay quiet."),
                div()
                    .min_h(px(140.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(ROW_RADIUS))
                    .bg(p.card)
                    .border_1()
                    .border_color(p.border)
                    .child(Textarea::new(&form.instructions))
                    .into_any_element(),
            ))
            .when_some(form.error.clone(), |this, error| {
                this.child(
                    h_flex()
                        .gap(px(8.0))
                        .items_start()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(ROW_RADIUS))
                        .bg(p.danger.opacity(0.08))
                        .border_1()
                        .border_color(p.danger.opacity(0.25))
                        .text_size(px(12.5))
                        .text_color(p.danger)
                        .child(div().pt(px(1.0)).child(icon(ICON_ALERT, 14.0, p.danger)))
                        .child(div().flex_1().min_w_0().child(error)),
                )
            })
            .child(
                h_flex()
                    .gap(px(8.0))
                    .justify_end()
                    .child(secondary_button(
                        p,
                        "ghost-habit-form-cancel",
                        None,
                        "Cancel",
                        |view: &mut Self, _, cx| {
                            view.habit_form = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(save),
            )
            .into_any_element()
    }

    fn render_habits(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        if let Some(form) = &self.habit_form {
            return self.render_habit_form(p, form, cx);
        }
        if let Some(Drill::Habit(id)) = &self.drill {
            if let Some(habit) = self.habits.iter().find(|habit| text(habit, "id") == *id) {
                return self.render_habit_detail(p, habit, cx);
            }
        }
        let add = secondary_button(
            p,
            "ghost-habit-new",
            Some(ICON_PLUS),
            "New habit",
            |view: &mut Self, window, cx| view.start_habit_form(window, cx),
            cx,
        )
        .into_any_element();
        if self.habits.is_empty() {
            return empty_state(
                p,
                ICON_CLOCK,
                "No habits yet",
                "A habit is a routine the Ghost runs on a schedule: a morning summary, an hourly check. It messages you only when there is something to tell.",
                Some(add),
            );
        }
        let today = chrono::Local::now().date_naive();
        let is_today = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .is_ok_and(|time| time.with_timezone(&chrono::Local).date_naive() == today)
        };
        let mut running = Vec::new();
        let mut up_next = Vec::new();
        let mut done_today = Vec::new();
        let mut later = Vec::new();
        for habit in &self.habits {
            let id = text(habit, "id");
            let ran_today = self
                .habit_runs(&id)
                .first()
                .is_some_and(|run| is_today(&text(run, "createdAt")));
            if self.active_run(&id).is_some() || self.run_starting.as_deref() == Some(id.as_str()) {
                running.push(habit);
            } else if habit["enabled"].as_bool() == Some(true)
                && is_today(&text(habit, "nextRunAt"))
            {
                up_next.push(habit);
            } else if ran_today {
                done_today.push(habit);
            } else {
                later.push(habit);
            }
        }
        let mut list = v_flex().w_full().gap(px(2.0)).px(px(8.0));
        for (heading, habits) in [
            ("Running now", running),
            ("Up next today", up_next),
            ("Done today", done_today),
            ("Other habits", later),
        ] {
            if habits.is_empty() {
                continue;
            }
            list = list.child(
                h_flex()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .pt(px(12.0))
                    .pb(px(4.0))
                    .text_size(px(12.5))
                    .child(div().text_color(p.foreground.opacity(0.75)).child(heading))
                    .child(div().text_color(p.muted).child(habits.len().to_string())),
            );
            for habit in habits {
                let id = text(habit, "id");
                let is_running = self.active_run(&id).is_some()
                    || self.run_starting.as_deref() == Some(id.as_str());
                let enabled = habit["enabled"].as_bool() == Some(true);
                let detail = if is_running {
                    format!(
                        "{} · started {}",
                        habit_schedule_label(&habit["schedule"]),
                        self.active_run(&id)
                            .map(|run| started_ago(&text(run, "createdAt")))
                            .unwrap_or_else(|| "just now".to_string())
                    )
                } else {
                    let last = self
                        .habit_runs(&id)
                        .first()
                        .map(|run| format!(" · {}", habit_run_outcome_label(&text(run, "status"))))
                        .unwrap_or_default();
                    format!(
                        "{}{}{last}",
                        habit_schedule_label(&habit["schedule"]),
                        if enabled { "" } else { " · off" }
                    )
                };
                let marker = if is_running {
                    spinner(
                        SharedString::from(format!("ghost-habit-spin-{id}")),
                        16.0,
                        p.accent,
                    )
                } else {
                    icon(
                        ICON_CLOCK,
                        16.0,
                        if enabled {
                            p.muted
                        } else {
                            p.muted.opacity(0.5)
                        },
                    )
                    .into_any_element()
                };
                list = list.child(
                    Self::row(p, SharedString::from(format!("ghost-habit-{id}")))
                        .child(marker)
                        .child(Self::two_lines(p, text(habit, "name"), detail))
                        .when(is_running, |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.0))
                                    .text_color(p.accent)
                                    .child("Running"),
                            )
                        })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.set_drill(Some(Drill::Habit(id.clone())), cx)
                        })),
                );
            }
        }
        v_flex()
            .w_full()
            .gap(px(4.0))
            .pt(px(12.0))
            .child(h_flex().px(px(16.0)).child(add))
            .child(list)
            .into_any_element()
    }
}

impl Render for NativeGhostOverviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = AutomatePalette::resolve(self.glass);
        let body = if self.ghost_id.is_none() {
            empty_state(
                &p,
                ICON_THREADS,
                "No Ghost selected",
                "Open a Ghost's Overview from its row in the sidebar.",
                None,
            )
        } else if !self.loaded {
            h_flex()
                .gap(px(8.0))
                .p(px(16.0))
                .items_center()
                .text_size(px(13.0))
                .text_color(p.muted)
                .child(spinner("ghost-overview-loading", 14.0, p.muted))
                .child("Loading…")
                .into_any_element()
        } else {
            match self.tab {
                Tab::Threads => self.render_threads(&p, cx),
                Tab::Documents => self.render_documents(&p, cx),
                Tab::Habits => self.render_habits(&p, cx),
            }
        };
        v_flex()
            .id("ghost-overview")
            .size_full()
            .min_h_0()
            .bg(p.page)
            .text_color(p.foreground)
            .font_family(p.font.clone())
            .child(self.render_header(&p, cx))
            .child(self.render_tabs(&p, cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .px(px(16.0))
                        .pb(px(8.0))
                        .text_size(px(12.5))
                        .text_color(p.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .id("ghost-overview-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pb(px(16.0))
                    .child(body),
            )
    }
}

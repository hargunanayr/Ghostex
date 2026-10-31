//! Native GPUI Handoff / Export dialog, the desktop twin of the React
//! `ExportTranscriptModal` in packages/core-ui/export-transcript-result-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:TranscriptExport 2026-09-15 DECISION:
//! User: the React app modals do not fill their GPUI child window and need a hand-tuned window height, so they are being rebuilt in GPUI one at a time, starting with Handoff / Export. The native dialog must match the React one 1 to 1: the same layout, copy, colors, states and behaviour in both appearances. It measures its own first layout and sizes the window to it instead of trusting a constant.
//! SEE-ALSO: packages/core-ui/export-transcript-result-modal.tsx (deleted 2026-10-01) and packages/core-ui/styles/modals.css (the React twin and the `.export-transcript-*` rules mirrored below), apps/desktop/src/app/window/native_modal_kit/ (shared chrome and controls), apps/desktop/src/app/export_transcript_modal_lifecycle.rs (open, close, sidebar bridge), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// The React dialog opens on the Rename Session width (`APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_WIDTH`).
pub(crate) const EXPORT_TRANSCRIPT_MODAL_WIDTH: f32 = 570.0;
/// First-frame height only. The window is resized to the measured layout as soon as the first prepaint reports it.
pub(crate) const EXPORT_TRANSCRIPT_MODAL_INITIAL_HEIGHT: f32 = 520.0;

const ICON_USER_SHARE: &str = "modals/export-transcript/user-share.svg";
const ICON_MARKDOWN: &str = "modals/export-transcript/markdown.svg";
const ICON_CHECK_CARD: &str = "modals/export-transcript/check-card.svg";
const ICON_CHECK_BUTTON: &str = "modals/export-transcript/check-button.svg";
const ICON_COPY: &str = "modals/export-transcript/copy.svg";
const ICON_FOLDER_SEARCH: &str = "modals/export-transcript/folder-search.svg";
const ICON_CIRCLE_CHECK: &str = "modals/export-transcript/circle-check-filled.svg";
const ICON_SERVER: &str = "modals/add-project/server.svg";

const TITLE: &str = "Handoff / Export";
const DESCRIPTION: &str = "Ghostex writes this conversation to a Markdown file. Pick what to do with it and what to include.";
const HANDOFF_CARD_TITLE: &str = "Handoff to an agent";
const HANDOFF_CARD_DESCRIPTION: &str = "Start a new conversation with the handover attached.";
const ADVANCED_CARD_TITLE: &str = "Advanced Handoff";
const ADVANCED_CARD_DESCRIPTION: &str = "Continue in another project, on this computer or another.";
const EXPORT_CARD_TITLE: &str = "Export to Markdown";
const EXPORT_CARD_DESCRIPTION: &str = "Save the conversation as a file and copy its path.";
const CONTINUE_WITH: &str = "Continue with";
const SELECT_AGENT_PLACEHOLDER: &str = "Select agent";
const COMPUTER: &str = "Computer";
const SELECT_COMPUTER_PLACEHOLDER: &str = "Select computer";
const PROJECT: &str = "Project";
const SELECT_PROJECT_PLACEHOLDER: &str = "Select project";
const NO_COMPUTER_HINT: &str =
    "No computer is connected. Connect one from the sidebar's computer tabs to hand off to it.";
const BRANCH_NOTICE_TITLE: &str = "Please note the difference in branches";
const BRANCH_NOTICE_SUBTITLE: &str =
    "We're just handing off the session, not switching the branch there";
const EXPORT_HINT: &str = "The file is saved in the Ghostex exports folder.";
const INCLUDE: &str = "Include";
const SAVED_AS_MARKDOWN: &str = "Saved as Markdown";
const OPEN_FILE_LOCATION: &str = "Open Location";
const EXPORT_FAILED: &str = "The transcript export failed.";

/// What the user wants to do with the written file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportTranscriptMode {
    Handoff,
    /// CDXC:TranscriptExport 2026-10-25 DECISION:
    /// User (Q542): "a 3rd radio card added 2nd there after handoff to agent, call it 'Handoff to Remote'; we should have those extra dropdowns to pick machine and project there only in that case". The Computer and Project selects exist only in this mode, beside the same agent choice Handoff to an agent has; the new session starts on that computer, in that project, with the transcript carried there. Every direction is offered (Q547): this computer to a remote, a remote to this computer, and remote to remote; the session's own computer is not a choice here, since Handoff to an agent already covers it.
    /// CDXC:TranscriptExport 2026-10-31 DECISION:
    /// User (Q865): "we should just be able to handoff to another project in the same machine using the 'handoff to remote' option but by changing it a bit (rename it to 'Advanced Handoff')". The session's own computer is now a choice too, listing its other projects; that is how the Unified sidebar, which has no drag, reaches another project. The saved mode stays `handoffRemote`, so a saved choice survives the rename.
    HandoffAdvanced,
    Export,
}

impl ExportTranscriptMode {
    fn is_handoff(self) -> bool {
        matches!(self, Self::Handoff | Self::HandoffAdvanced)
    }
}

/// The include-toggles; user and agent messages are never optional, so only
/// the three optional record families are here. Defaults mirror the daemon's
/// historical selection: commands and patches in, reasoning out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptIncludeOptions {
    pub(crate) commands: bool,
    pub(crate) patches: bool,
    pub(crate) reasoning: bool,
}

impl Default for ExportTranscriptIncludeOptions {
    fn default() -> Self {
        Self {
            commands: true,
            patches: true,
            reasoning: false,
        }
    }
}

/// The user's last mode and include combination. A per-client UI preference,
/// so repeat exports reopen exactly as they were left without involving gxserver.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExportTranscriptModalPrefs {
    pub(crate) mode: Option<ExportTranscriptMode>,
    pub(crate) include: ExportTranscriptIncludeOptions,
}

pub(crate) fn load_export_transcript_modal_prefs(path: &Path) -> ExportTranscriptModalPrefs {
    let Some(value) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return ExportTranscriptModalPrefs::default();
    };
    let defaults = ExportTranscriptIncludeOptions::default();
    let flag = |key: &str, default: bool| {
        value
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    };
    ExportTranscriptModalPrefs {
        mode: match value.get("mode").and_then(serde_json::Value::as_str) {
            Some("export") => Some(ExportTranscriptMode::Export),
            Some("handoff") => Some(ExportTranscriptMode::Handoff),
            Some("handoffRemote") => Some(ExportTranscriptMode::HandoffAdvanced),
            _ => None,
        },
        include: ExportTranscriptIncludeOptions {
            commands: flag("includeCommands", defaults.commands),
            patches: flag("includePatches", defaults.patches),
            reasoning: flag("includeReasoning", defaults.reasoning),
        },
    }
}

pub(crate) fn persist_export_transcript_modal_prefs(
    path: &Path,
    prefs: ExportTranscriptModalPrefs,
) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "mode": match prefs.mode {
            Some(ExportTranscriptMode::Export) => "export",
            Some(ExportTranscriptMode::HandoffAdvanced) => "handoffRemote",
            _ => "handoff",
        },
        "includeCommands": prefs.include.commands,
        "includePatches": prefs.include.patches,
        "includeReasoning": prefs.include.reasoning,
    });
    let _ = std::fs::write(path, payload.to_string());
}

/// A configured agent that can take the handoff (the React modal only offers
/// agents whose HUD button carries a launch command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    /// The chat model picker's family for this agent's logo (`claude`, `codex`, …), when it has one.
    pub(crate) family: Option<String>,
}

/// The chat model picker's pick of another agent's model, which the dialog hands over to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptHandoffTarget {
    pub(crate) family: String,
    pub(crate) model: String,
}

/// A computer Advanced Handoff can start the new session on, with its projects ranked for this
/// session (`ghostex_gx_core::handoff_target`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptComputer {
    /// `local` for this computer, else the remote machine id.
    pub(crate) machine_id: String,
    pub(crate) label: String,
    /// The computer the session runs on: its transcript is already there, and the session's own
    /// project is not among `projects`.
    pub(crate) is_source: bool,
    pub(crate) projects: Vec<ExportTranscriptProject>,
    pub(crate) preferred_project_id: Option<String>,
    /// The repository's name when this session's project has a git remote that no project on
    /// this computer matches: the dialog then offers to clone it there.
    pub(crate) missing_repository: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptProject {
    pub(crate) project_id: String,
    pub(crate) title: String,
    pub(crate) path: Option<String>,
    pub(crate) same_repo: bool,
}

/// Where Advanced Handoff starts the new session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptRemoteTarget {
    pub(crate) machine_id: String,
    pub(crate) project_id: String,
}

/// The dialog's picks, handed to the host when it closes for a clone and given back when it
/// reopens, so the user returns to the dialog as they left it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptPicks {
    pub(crate) mode: ExportTranscriptMode,
    pub(crate) include: ExportTranscriptIncludeOptions,
    pub(crate) agent_id: Option<String>,
    pub(crate) machine_id: Option<String>,
    pub(crate) project_id: Option<String>,
}

/// The computer Advanced Handoff goes to: the picked one while it is still offered, else the
/// first.
pub(crate) fn handoff_computer<'a>(
    computers: &'a [ExportTranscriptComputer],
    selected_machine_id: Option<&str>,
) -> Option<&'a ExportTranscriptComputer> {
    selected_machine_id
        .and_then(|id| computers.iter().find(|computer| computer.machine_id == id))
        .or_else(|| computers.first())
}

/// The project on that computer: the pick when it is one of its projects, else the one with the
/// session's repository, else none (the user must pick one).
pub(crate) fn handoff_project<'a>(
    computer: &'a ExportTranscriptComputer,
    selected_project_id: Option<&str>,
) -> Option<&'a ExportTranscriptProject> {
    let find = |id: &str| {
        computer
            .projects
            .iter()
            .find(|project| project.project_id == id)
    };
    selected_project_id
        .and_then(find)
        .or_else(|| computer.preferred_project_id.as_deref().and_then(find))
}

/// Advanced Handoff's button: it names where the session goes (Q862), with the computer when it
/// leaves this one.
fn advanced_handoff_label(
    computer: &ExportTranscriptComputer,
    project: &ExportTranscriptProject,
) -> String {
    if computer.is_source {
        format!("Handoff to {}", project.title)
    } else {
        format!("Handoff to {} on {}", project.title, computer.label)
    }
}

/// A Project select row: the title, the same-repository mark, and the folder.
fn project_label(project: &ExportTranscriptProject) -> String {
    let mut label = project.title.clone();
    if project.same_repo {
        label.push_str(" (same repo)");
    }
    if let Some(path) = project.path.as_deref().filter(|path| !path.is_empty()) {
        label.push_str(" · ");
        label.push_str(path);
    }
    label
}

/// The agent a model picker handoff goes to: the first agent of the picked family in the list the
/// dialog shows, or none. Never the session's own agent standing in for a family that is missing.
pub(crate) fn handoff_target_agent<'a>(
    agents: &'a [ExportTranscriptAgent],
    family: &str,
) -> Option<&'a ExportTranscriptAgent> {
    agents
        .iter()
        .find(|agent| agent.family.as_deref() == Some(family))
}

/// The family's name as the dialog says it.
fn family_display_name(family: &str) -> &str {
    match family {
        "claude" => "Claude",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "antigravity" => "Antigravity CLI",
        other => other,
    }
}

/// The dialog's lifecycle, owned by the host: choose what to include, watch
/// the daemon write the file, then follow up on the result. `Failed` keeps the
/// dialog open with the daemon's message and a way back to the export.
#[derive(Clone, Debug)]
pub(crate) enum ExportTranscriptStage {
    Options,
    Exporting,
    Done {
        agent_id: Option<String>,
        can_reveal: bool,
        path: String,
    },
    Failed {
        message: String,
    },
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending any command other than `RunExport`.
pub(crate) enum ExportTranscriptModalCommand {
    /// `carry` asks for the file's text too, which Advanced Handoff takes to another computer.
    RunExport {
        include: ExportTranscriptIncludeOptions,
        carry: bool,
    },
    StartConversation {
        agent_id: String,
        /// `None` starts it beside the session (Handoff to an agent).
        target: Option<ExportTranscriptRemoteTarget>,
    },
    /// The Computer or Project select changed: the host reads both checkouts' branches and
    /// answers with [`GpuiExportTranscriptModalWindow::set_branch_notice`].
    TargetChanged(ExportTranscriptRemoteTarget),
    /// Clone the session's repository on `machine_id` (or add a project there when it has no
    /// remote). The dialog has removed its window; the host reopens it with `picks` afterwards.
    AddProjectOn {
        machine_id: String,
        picks: ExportTranscriptPicks,
    },
    Cancel,
    Reveal,
    /// The exported file's path was just written to the clipboard; the host gives the copy feedback.
    PathCopied,
}

pub(crate) type ExportTranscriptModalHost = Rc<dyn Fn(ExportTranscriptModalCommand, &mut App)>;

pub(crate) struct ExportTranscriptModalConfig {
    pub(crate) agents: Vec<ExportTranscriptAgent>,
    /// The exported session's own agent, preselected so "handoff to the same agent" is one click away.
    pub(crate) default_agent_id: Option<String>,
    /// The chat model picker's pick. Its agent is resolved from `agents` (and again whenever they refresh), and it outranks `default_agent_id` and the finished export's agent.
    pub(crate) handoff_target: Option<ExportTranscriptHandoffTarget>,
    pub(crate) palette: ModalPalette,
    pub(crate) prefs_path: Option<PathBuf>,
    /// Overrides the remembered mode on open; the demo uses it to show one branch.
    pub(crate) initial_mode: Option<ExportTranscriptMode>,
    /// The computers Advanced Handoff offers, the session's own first.
    pub(crate) computers: Vec<ExportTranscriptComputer>,
    /// The picks to reopen with after a clone (`AddProjectOn`), over the remembered prefs.
    pub(crate) restore: Option<ExportTranscriptPicks>,
}

pub(crate) struct GpuiExportTranscriptModalWindow {
    host: ExportTranscriptModalHost,
    palette: ModalPalette,
    prefs_path: Option<PathBuf>,
    agents: Vec<ExportTranscriptAgent>,
    default_agent_id: Option<String>,
    handoff_target: Option<ExportTranscriptHandoffTarget>,
    mode: ExportTranscriptMode,
    include: ExportTranscriptIncludeOptions,
    selected_agent_id: Option<String>,
    stage: ExportTranscriptStage,
    /// Set by a Handoff run so the done stage starts the conversation instead of showing the path.
    handoff_requested: bool,
    copied: bool,
    copied_generation: u64,
    agent_select: ModalSelect,
    computers: Vec<ExportTranscriptComputer>,
    selected_machine_id: Option<String>,
    selected_project_id: Option<String>,
    computer_select: ModalSelect,
    project_select: ModalSelect,
    /// The branch notice's first line, for the target it was read for.
    branch_notice: Option<(ExportTranscriptRemoteTarget, String)>,
    /// The target the host was last asked about, so a repeat pick asks nothing.
    announced_target: Option<ExportTranscriptRemoteTarget>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _click_away: Vec<gpui::Subscription>,
}

impl GpuiExportTranscriptModalWindow {
    pub(crate) fn new(
        config: ExportTranscriptModalConfig,
        host: ExportTranscriptModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = config
            .prefs_path
            .as_deref()
            .map(load_export_transcript_modal_prefs)
            .unwrap_or_default();
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let restore = config.restore;
        let mut this = Self {
            host,
            palette: config.palette,
            prefs_path: config.prefs_path,
            agents: config.agents,
            default_agent_id: config.default_agent_id,
            handoff_target: config.handoff_target,
            mode: restore
                .as_ref()
                .map(|picks| picks.mode)
                .or(config.initial_mode)
                .or(prefs.mode)
                .unwrap_or(ExportTranscriptMode::Handoff),
            include: restore
                .as_ref()
                .map(|picks| picks.include)
                .unwrap_or(prefs.include),
            selected_agent_id: restore.as_ref().and_then(|picks| picks.agent_id.clone()),
            stage: ExportTranscriptStage::Options,
            handoff_requested: false,
            copied: false,
            copied_generation: 0,
            agent_select: ModalSelect::new(),
            computers: config.computers,
            selected_machine_id: restore.as_ref().and_then(|picks| picks.machine_id.clone()),
            selected_project_id: restore.as_ref().and_then(|picks| picks.project_id.clone()),
            computer_select: ModalSelect::new(),
            project_select: ModalSelect::new(),
            branch_notice: None,
            announced_target: None,
            fit: ModalFit::new(),
            focus_handle,
            _click_away: super::popup_dismissal::close_app_modal_on_click_away(window, cx),
        };
        this.announce_target(cx);
        this
    }

    /// The host's answer to `TargetChanged`: the notice's first line, or `None` when both
    /// checkouts are on one branch (or either is unknown).
    pub(crate) fn set_branch_notice(
        &mut self,
        target: ExportTranscriptRemoteTarget,
        line: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.remote_target().as_ref() != Some(&target) {
            return;
        }
        self.branch_notice = line.map(|line| (target, line));
        self.fit.refit();
        cx.notify();
    }

    /// The computer Advanced Handoff goes to: the pick, else the first one offered.
    fn effective_computer(&self) -> Option<&ExportTranscriptComputer> {
        handoff_computer(&self.computers, self.selected_machine_id.as_deref())
    }

    fn effective_computer_index(&self) -> Option<usize> {
        let computer = self.effective_computer()?;
        self.computers
            .iter()
            .position(|candidate| candidate.machine_id == computer.machine_id)
    }

    /// The project on that computer: the pick when it is one of its projects, else the one with
    /// this session's repository.
    fn effective_project(&self) -> Option<&ExportTranscriptProject> {
        handoff_project(
            self.effective_computer()?,
            self.selected_project_id.as_deref(),
        )
    }

    fn effective_project_index(&self) -> Option<usize> {
        let computer = self.effective_computer()?;
        let project = self.effective_project()?;
        computer
            .projects
            .iter()
            .position(|candidate| candidate.project_id == project.project_id)
    }

    fn remote_target(&self) -> Option<ExportTranscriptRemoteTarget> {
        Some(ExportTranscriptRemoteTarget {
            machine_id: self.effective_computer()?.machine_id.clone(),
            project_id: self.effective_project()?.project_id.clone(),
        })
    }

    /// Tells the host which checkout Advanced Handoff now points at, once per change.
    fn announce_target(&mut self, cx: &mut Context<Self>) {
        let target = (self.mode == ExportTranscriptMode::HandoffAdvanced)
            .then(|| self.remote_target())
            .flatten();
        if target == self.announced_target {
            return;
        }
        self.announced_target = target.clone();
        if self
            .branch_notice
            .as_ref()
            .is_some_and(|(notice_target, _)| Some(notice_target) != target.as_ref())
        {
            self.branch_notice = None;
        }
        if let Some(target) = target {
            (self.host)(ExportTranscriptModalCommand::TargetChanged(target), cx);
        }
    }

    fn picks(&self) -> ExportTranscriptPicks {
        ExportTranscriptPicks {
            mode: self.mode,
            include: self.include,
            agent_id: self.selected_agent_id.clone(),
            machine_id: self
                .effective_computer()
                .map(|computer| computer.machine_id.clone()),
            project_id: self.selected_project_id.clone(),
        }
    }

    /// A fresh agent list from the host (the HUD read that finishes after open).
    pub(crate) fn set_agents(
        &mut self,
        agents: Vec<ExportTranscriptAgent>,
        cx: &mut Context<Self>,
    ) {
        if self.agents == agents {
            return;
        }
        self.agents = agents;
        if self.agents.is_empty() {
            self.agent_select.close();
        }
        cx.notify();
    }

    /// The store's answer to `RunExport` (gx_store/git/export_transcript.rs).
    pub(crate) fn receive_result(
        &mut self,
        ok: bool,
        path: Option<String>,
        can_reveal: bool,
        agent_id: Option<String>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handoff_requested = std::mem::take(&mut self.handoff_requested);
        match (ok, path) {
            (true, Some(path)) => {
                self.stage = ExportTranscriptStage::Done {
                    agent_id,
                    can_reveal,
                    path,
                };
                if handoff_requested
                    && self.effective_mode().is_handoff()
                    && let Some(agent) = self.effective_agent()
                {
                    let agent_id = agent.agent_id.clone();
                    let target = match self.effective_mode() {
                        ExportTranscriptMode::HandoffAdvanced => self.remote_target(),
                        _ => None,
                    };
                    self.close_window_and_send(
                        ExportTranscriptModalCommand::StartConversation { agent_id, target },
                        window,
                        cx,
                    );
                    return;
                }
            }
            _ => {
                self.stage = ExportTranscriptStage::Failed {
                    message: error
                        .map(|message| message.trim().to_string())
                        .filter(|message| !message.is_empty())
                        .unwrap_or_else(|| EXPORT_FAILED.to_string()),
                };
            }
        }
        cx.notify();
    }

    fn effective_mode(&self) -> ExportTranscriptMode {
        if self.agents.is_empty() {
            ExportTranscriptMode::Export
        } else {
            self.mode
        }
    }

    fn effective_agent(&self) -> Option<&ExportTranscriptAgent> {
        let done_agent_id = match &self.stage {
            ExportTranscriptStage::Done { agent_id, .. } => agent_id.as_deref(),
            _ => None,
        };
        let selected = self
            .selected_agent_id
            .as_deref()
            .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id));
        // CDXC:TranscriptExport 2026-10-25 WHY:
        // A model picker handoff resolves its agent from the list this dialog shows. Until 2026-10-25 the host matched it against a second agent list and the dialog fell back to the session's own agent when that id was not in its list, so picking GPT 6 Astra in a Claude session said "Handoff to Claude" and started Claude. A family with no agent here leaves the choice empty and says so (`target_unavailable_hint`).
        if let Some(target) = &self.handoff_target {
            return selected.or_else(|| handoff_target_agent(&self.agents, &target.family));
        }
        selected
            .or_else(|| {
                done_agent_id
                    .or(self.default_agent_id.as_deref())
                    .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id))
            })
            .or_else(|| self.agents.first())
    }

    /// What the dialog says when the picked model's agent is not one it can start.
    fn target_unavailable_hint(&self) -> Option<String> {
        let target = self.handoff_target.as_ref()?;
        if self.agents.is_empty()
            || !self.effective_mode().is_handoff()
            || self.effective_agent().is_some()
        {
            return None;
        }
        let family = family_display_name(&target.family);
        Some(format!(
            "No {family} agent is set up here, so this can't hand off to {}. Pick another agent, or add {family} in Settings > Agents.",
            target.model
        ))
    }

    fn effective_agent_index(&self) -> Option<usize> {
        self.effective_agent()
            .and_then(|agent| self.agents.iter().position(|candidate| candidate == agent))
    }

    fn is_done(&self) -> bool {
        matches!(self.stage, ExportTranscriptStage::Done { .. })
    }

    fn is_exporting(&self) -> bool {
        matches!(self.stage, ExportTranscriptStage::Exporting)
    }

    fn show_result(&self) -> bool {
        self.is_done() && self.effective_mode() == ExportTranscriptMode::Export
    }

    fn busy(&self) -> bool {
        self.is_exporting() || (self.is_done() && self.effective_mode().is_handoff())
    }

    fn controls_disabled(&self) -> bool {
        self.busy() || self.is_done()
    }

    fn can_run(&self) -> bool {
        !self.is_done()
            && !self.is_exporting()
            && match self.effective_mode() {
                ExportTranscriptMode::Export => true,
                ExportTranscriptMode::Handoff => self.effective_agent().is_some(),
                ExportTranscriptMode::HandoffAdvanced => {
                    self.effective_agent().is_some() && self.remote_target().is_some()
                }
            }
    }

    fn primary_label(&self) -> String {
        if self.is_exporting() {
            return "Exporting…".to_string();
        }
        if self.is_done() && self.effective_mode().is_handoff() {
            return "Starting…".to_string();
        }
        if matches!(self.stage, ExportTranscriptStage::Failed { .. }) {
            return "Try Again".to_string();
        }
        match self.effective_mode() {
            ExportTranscriptMode::Handoff => match self.effective_agent() {
                Some(agent) => format!("Handoff to {}", agent.name),
                None => "Handoff".to_string(),
            },
            ExportTranscriptMode::HandoffAdvanced => {
                match (self.effective_computer(), self.effective_project()) {
                    (Some(computer), Some(project)) => advanced_handoff_label(computer, project),
                    _ => "Handoff".to_string(),
                }
            }
            ExportTranscriptMode::Export => "Export".to_string(),
        }
    }

    fn persist_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            persist_export_transcript_modal_prefs(
                path,
                ExportTranscriptModalPrefs {
                    mode: Some(self.mode),
                    include: self.include,
                },
            );
        }
    }

    fn choose_mode(&mut self, mode: ExportTranscriptMode, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.mode = mode;
        self.close_menus();
        self.persist_prefs();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn close_menus(&mut self) {
        self.agent_select.close();
        self.computer_select.close();
        self.project_select.close();
    }

    fn toggle_include(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        match index {
            0 => self.include.commands = !self.include.commands,
            1 => self.include.patches = !self.include.patches,
            _ => self.include.reasoning = !self.include.reasoning,
        }
        self.persist_prefs();
        cx.notify();
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if !self.can_run() {
            return;
        }
        self.close_menus();
        self.handoff_requested = self.effective_mode().is_handoff();
        self.stage = ExportTranscriptStage::Exporting;
        cx.notify();
        (self.host)(
            ExportTranscriptModalCommand::RunExport {
                include: self.include,
                carry: self.effective_mode() == ExportTranscriptMode::HandoffAdvanced
                    && self
                        .effective_computer()
                        .is_some_and(|computer| !computer.is_source),
            },
            cx,
        );
    }

    fn copy_path(&mut self, cx: &mut Context<Self>) {
        let ExportTranscriptStage::Done { path, .. } = &self.stage else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
        (self.host)(ExportTranscriptModalCommand::PathCopied, cx);
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1_500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn close_window_and_send(
        &mut self,
        command: ExportTranscriptModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(ExportTranscriptModalCommand::Cancel, window, cx);
    }

    fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(ExportTranscriptModalCommand::Reveal, window, cx);
    }

    fn toggle_agent_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.agents.is_empty() {
            return;
        }
        let selected = self.effective_agent_index();
        self.computer_select.close();
        self.project_select.close();
        self.agent_select.toggle(selected);
        cx.notify();
    }

    fn toggle_computer_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.computers.is_empty() {
            return;
        }
        let selected = self.effective_computer_index();
        self.agent_select.close();
        self.project_select.close();
        self.computer_select.toggle(selected);
        cx.notify();
    }

    fn choose_computer(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(computer) = self.computers.get(index) {
            if self.selected_machine_id.as_deref() != Some(computer.machine_id.as_str()) {
                self.selected_project_id = None;
            }
            self.selected_machine_id = Some(computer.machine_id.clone());
        }
        self.computer_select.close();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn project_count(&self) -> usize {
        self.effective_computer()
            .map(|computer| computer.projects.len())
            .unwrap_or(0)
    }

    fn toggle_project_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.project_count() == 0 {
            return;
        }
        let selected = self.effective_project_index();
        self.agent_select.close();
        self.computer_select.close();
        self.project_select.toggle(selected);
        cx.notify();
    }

    fn choose_project(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(project) = self
            .effective_computer()
            .and_then(|computer| computer.projects.get(index))
        {
            self.selected_project_id = Some(project.project_id.clone());
        }
        self.project_select.close();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    /// Q544: the Add Project clone flow on that computer, prefilled with this session's repository.
    fn add_project_on_computer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(machine_id) = self
            .effective_computer()
            .map(|computer| computer.machine_id.clone())
        else {
            return;
        };
        let picks = self.picks();
        self.close_window_and_send(
            ExportTranscriptModalCommand::AddProjectOn { machine_id, picks },
            window,
            cx,
        );
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(agent) = self.agents.get(index) {
            self.selected_agent_id = Some(agent.agent_id.clone());
        }
        self.agent_select.close();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match self.agent_select.handle_key(key, self.agents.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match self.computer_select.handle_key(key, self.computers.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_computer(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        let project_count = self.project_count();
        match self.project_select.handle_key(key, project_count) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_project(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" => {
                if event.is_held {
                    return;
                }
                if self.show_result() {
                    self.copy_path(cx);
                } else {
                    self.run(cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_mode_card(
        &self,
        index: usize,
        mode: ExportTranscriptMode,
        icon_path: &'static str,
        title: &'static str,
        description: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let selected = self.effective_mode() == mode;
        let disabled = self.controls_disabled();
        h_flex()
            .id(("export-transcript-mode-card", index))
            .relative()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .items_start()
            .gap(px(10.0))
            .pt(px(10.0))
            .pr(px(12.0))
            .pb(px(11.0))
            .pl(px(10.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if selected {
                p.card_selected_border()
            } else {
                p.hairline
            }))
            .bg(hsla(if selected {
                p.card_selected_background()
            } else {
                p.raised
            }))
            .when(disabled, |this| this.opacity(0.6).cursor_default())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.raised_hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.choose_mode(mode, cx);
                    }))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(28.0))
                    .rounded(px(8.0))
                    .bg(hsla(if selected {
                        p.primary
                    } else {
                        p.chip_background()
                    }))
                    .child(modal_icon(
                        icon_path,
                        16.0,
                        if selected {
                            p.primary_foreground
                        } else {
                            p.muted
                        },
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .pr(px(16.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(16.9))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .text_color(hsla(p.muted))
                            .child(description),
                    ),
            )
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(10.0))
                        .right(px(10.0))
                        .child(modal_icon(ICON_CHECK_CARD, 14.0, p.foreground)),
                )
            })
            .into_any_element()
    }

    fn render_mode_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        // CSS grid stretches both cards to the taller one; gpui-component's row helper centers items.
        let mut grid = h_flex().w_full().items_stretch().gap(px(8.0));
        if !self.agents.is_empty() {
            grid = grid
                .child(self.render_mode_card(
                    0,
                    ExportTranscriptMode::Handoff,
                    ICON_USER_SHARE,
                    HANDOFF_CARD_TITLE,
                    HANDOFF_CARD_DESCRIPTION,
                    cx,
                ))
                .child(self.render_mode_card(
                    2,
                    ExportTranscriptMode::HandoffAdvanced,
                    ICON_SERVER,
                    ADVANCED_CARD_TITLE,
                    ADVANCED_CARD_DESCRIPTION,
                    cx,
                ));
        }
        grid.child(self.render_mode_card(
            1,
            ExportTranscriptMode::Export,
            ICON_MARKDOWN,
            EXPORT_CARD_TITLE,
            EXPORT_CARD_DESCRIPTION,
            cx,
        ))
        .into_any_element()
    }

    /// Both branches share one height so the fitted window never clips when the mode changes.
    fn render_agent_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        if self.effective_mode() == ExportTranscriptMode::Export {
            return h_flex()
                .min_h(px(MODAL_CONTROL_HEIGHT))
                .items_center()
                .child(modal_hint(&p, EXPORT_HINT))
                .into_any_element();
        }
        let value = self.effective_agent().map(|agent| agent.name.clone());
        let remote = self.effective_mode() == ExportTranscriptMode::HandoffAdvanced;
        // In Advanced Handoff the three labels share one width so the selects line up.
        let label = |text: &'static str| {
            let title = modal_section_title(&p, text).flex_shrink_0();
            if remote { title.w(px(96.0)) } else { title }
        };
        let agent_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.agent_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(CONTINUE_WITH))
            .child(modal_select_trigger(
                &p,
                &self.agent_select,
                "export-transcript-agent-select",
                value,
                SELECT_AGENT_PLACEHOLDER,
                self.controls_disabled(),
                |this, _window, cx| this.toggle_agent_menu(cx),
                cx,
            ));
        if !remote {
            return agent_row.into_any_element();
        }
        let computer = self.effective_computer();
        let project_value = self.effective_project().map(project_label);
        let computer_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.computer_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(COMPUTER))
            .child(modal_select_trigger(
                &p,
                &self.computer_select,
                "export-transcript-computer-select",
                computer.map(|computer| computer.label.clone()),
                SELECT_COMPUTER_PLACEHOLDER,
                self.controls_disabled() || self.computers.is_empty(),
                |this, _window, cx| this.toggle_computer_menu(cx),
                cx,
            ));
        let project_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.project_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(PROJECT))
            .child(modal_select_trigger(
                &p,
                &self.project_select,
                "export-transcript-project-select",
                project_value,
                SELECT_PROJECT_PLACEHOLDER,
                self.controls_disabled() || self.project_count() == 0,
                |this, _window, cx| this.toggle_project_menu(cx),
                cx,
            ));
        let mut rows = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(agent_row)
            .child(computer_row)
            .child(project_row);
        if self.computers.is_empty() {
            rows = rows.child(modal_hint(&p, NO_COMPUTER_HINT));
        } else if let Some(computer) = computer
            && computer.projects.iter().all(|project| !project.same_repo)
        {
            rows = rows.child(self.render_missing_project(computer, cx));
        }
        if let Some((_, line)) = &self.branch_notice {
            rows = rows.child(self.render_branch_notice(line.clone()));
        }
        rows.into_any_element()
    }

    /// Q544: the other computer has no project with this repository (or no project at all).
    fn render_missing_project(
        &self,
        computer: &ExportTranscriptComputer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let (text, button) = match &computer.missing_repository {
            Some(repository) => (
                format!("{} doesn't have {repository} yet.", computer.label),
                "Clone it there…",
            ),
            None if computer.projects.is_empty() => (
                format!("{} has no projects yet.", computer.label),
                "Add a project there…",
            ),
            None => return div().into_any_element(),
        };
        h_flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .child(modal_hint(&p, text).flex_1().min_w_0())
            .child(modal_action_button(
                &p,
                "export-transcript-add-project",
                button,
                None,
                ModalButtonTone::Neutral,
                self.controls_disabled(),
                |this, window, cx| this.add_project_on_computer(window, cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_branch_notice(&self, line: String) -> AnyElement {
        let p = self.palette;
        modal_panel(&p)
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(BRANCH_NOTICE_TITLE),
            )
            .child(modal_hint(&p, line))
            .child(modal_hint(&p, BRANCH_NOTICE_SUBTITLE))
            .into_any_element()
    }

    fn render_include(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let disabled = self.controls_disabled();
        const ROWS: [(&str, &str); 3] = [
            (
                "Commands & output",
                "Terminal commands the agent ran, with the tail of their output.",
            ),
            ("File changes", "The patches the agent applied to files."),
            ("Reasoning", "The agent's own reasoning sections."),
        ];
        let values = [
            self.include.commands,
            self.include.patches,
            self.include.reasoning,
        ];
        let mut elements = vec![
            modal_section_title(&p, INCLUDE)
                .mt(px(4.0))
                .into_any_element(),
            modal_panel(&p)
                .children(
                    ROWS.iter()
                        .enumerate()
                        .map(|(index, (label, description))| {
                            modal_panel_row(
                                &p,
                                *label,
                                Some(*description),
                                modal_switch(&p, values[index], disabled),
                                index > 0,
                            )
                            .id(("export-transcript-toggle-row", index))
                            .when(!disabled, |this| {
                                this.cursor_pointer().on_click(cx.listener(
                                    move |this, _: &ClickEvent, _window, cx| {
                                        this.toggle_include(index, cx);
                                    },
                                ))
                            })
                        }),
                )
                .into_any_element(),
        ];
        if let ExportTranscriptStage::Failed { message } = &self.stage {
            elements.push(modal_error(&p, message.clone()));
        }
        elements
    }

    fn render_result(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (path, can_reveal) = match &self.stage {
            ExportTranscriptStage::Done {
                path, can_reveal, ..
            } => (path.clone(), *can_reveal),
            _ => (String::new(), false),
        };
        modal_panel(&p)
            .mt(px(4.0))
            .gap(px(10.0))
            .p(px(12.0))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .font_weight(FontWeight::MEDIUM)
                    .child(modal_icon(ICON_CIRCLE_CHECK, 16.0, p.success))
                    .child(SAVED_AS_MARKDOWN),
            )
            .child(
                modal_raised_box(&p)
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .gap(px(6.0))
                    .pt(px(8.0))
                    .pr(px(8.0))
                    .pb(px(8.0))
                    .pl(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .child(path),
                    )
                    .when(can_reveal, |this| {
                        this.child(
                            modal_icon_button(
                                &p,
                                "export-transcript-reveal",
                                ICON_FOLDER_SEARCH,
                                15.0,
                                |this, window, cx| this.reveal(window, cx),
                                cx,
                            )
                            .tooltip(|window, cx| {
                                Tooltip::new(OPEN_FILE_LOCATION).build(window, cx)
                            }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// CDXC:TranscriptExport 2026-09-15 DECISION:
    /// User: Handoff / Export is one page, not an options page followed by a result page, and "Handoff to an agent" and "Export to Markdown" are explicit choices on it ("Handoff to Remote" joined them between the two on 2026-10-25, see `ExportTranscriptMode::HandoffAdvanced`). The mode cards sit above the include-toggles; Handoff writes the file and starts the follow-up conversation in one click, while Export swaps the toggles for the saved path with Copy and Reveal. Starting a conversation never sends a prompt for the user: the mention is typed into the new agent's input and left unsubmitted. Reveal is omitted, not disabled, when the file lives on another machine.
    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(self.render_mode_grid(cx))
            .child(self.render_agent_row(cx));
        if let Some(hint) = self.target_unavailable_hint() {
            body = body.child(modal_hint(&self.palette, hint));
        }
        if self.show_result() {
            body = body.child(self.render_result(cx));
        } else {
            body = body.children(self.render_include(cx));
        }
        body.into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let show_result = self.show_result();
        let left = modal_action_button(
            &p,
            "export-transcript-cancel",
            if show_result { "Done" } else { "Cancel" },
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let right = if show_result {
            let copied = self.copied;
            modal_action_button(
                &p,
                "export-transcript-copy",
                if copied { "Path Copied" } else { "Copy Path" },
                Some(
                    modal_icon(
                        if copied { ICON_CHECK_BUTTON } else { ICON_COPY },
                        15.0,
                        p.primary_foreground,
                    )
                    .into_any_element(),
                ),
                ModalButtonTone::Primary,
                false,
                |this, _window, cx| this.copy_path(cx),
                cx,
            )
        } else {
            modal_action_button(
                &p,
                "export-transcript-primary",
                self.primary_label(),
                self.busy().then(|| modal_spinner(p.primary_foreground)),
                ModalButtonTone::Primary,
                !self.can_run(),
                |this, _window, cx| this.run(cx),
                cx,
            )
        };
        modal_footer(vec![left, right])
    }
}

impl Render for GpuiExportTranscriptModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let agent_menu = modal_select_menu(
            &p,
            &self.agent_select,
            "export-transcript-agent-menu",
            &names,
            self.effective_agent_index(),
            |this, index, _window, cx| this.choose_agent(index, cx),
            |this, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let computer_names: Vec<String> = self
            .computers
            .iter()
            .map(|computer| computer.label.clone())
            .collect();
        let computer_menu = modal_select_menu(
            &p,
            &self.computer_select,
            "export-transcript-computer-menu",
            &computer_names,
            self.effective_computer_index(),
            |this, index, _window, cx| this.choose_computer(index, cx),
            |this, _window, cx| {
                this.computer_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let project_names: Vec<String> = self
            .effective_computer()
            .map(|computer| computer.projects.iter().map(project_label).collect())
            .unwrap_or_default();
        let project_menu = modal_select_menu(
            &p,
            &self.project_select,
            "export-transcript-project-menu",
            &project_names,
            self.effective_project_index(),
            |this, index, _window, cx| this.choose_project(index, cx),
            |this, _window, cx| {
                this.project_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let menu = agent_menu.or(computer_menu).or(project_menu);
        let content = vec![
            modal_header(&p, TITLE, Some(DESCRIPTION)),
            self.render_body(cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-export-transcript-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            menu,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiExportTranscriptModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }

    /// Only the result closes on a click away; the options and a running export stay.
    fn keeps_open_on_click_away(&self, _window: &Window, _cx: &App) -> bool {
        matches!(
            self.stage,
            ExportTranscriptStage::Options | ExportTranscriptStage::Exporting
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(agent_id: &str, family: Option<&str>) -> ExportTranscriptAgent {
        ExportTranscriptAgent {
            agent_id: agent_id.to_string(),
            name: agent_id.to_string(),
            family: family.map(str::to_string),
        }
    }

    #[test]
    fn handoff_target_is_the_picked_family_not_the_sessions_agent() {
        let agents = [
            agent("claude", Some("claude")),
            agent("opencode", None),
            agent("codex", Some("codex")),
        ];
        assert_eq!(
            handoff_target_agent(&agents, "codex").map(|agent| agent.agent_id.as_str()),
            Some("codex")
        );
    }

    fn computer(
        machine_id: &str,
        projects: &[&str],
        preferred: Option<&str>,
    ) -> ExportTranscriptComputer {
        ExportTranscriptComputer {
            machine_id: machine_id.to_string(),
            label: machine_id.to_string(),
            is_source: false,
            projects: projects
                .iter()
                .map(|id| ExportTranscriptProject {
                    project_id: id.to_string(),
                    title: id.to_string(),
                    path: None,
                    same_repo: Some(*id) == preferred,
                })
                .collect(),
            preferred_project_id: preferred.map(str::to_string),
            missing_repository: None,
        }
    }

    #[test]
    fn remote_target_defaults_to_the_first_computer_and_its_matching_project() {
        let computers = [
            computer("studio", &["site", "ghostex"], Some("ghostex")),
            computer("linux", &["notes"], None),
        ];
        let first = handoff_computer(&computers, None).unwrap();
        assert_eq!(first.machine_id, "studio");
        assert_eq!(handoff_project(first, None).unwrap().project_id, "ghostex");
        // A computer that went away falls back to the first one offered.
        assert_eq!(
            handoff_computer(&computers, Some("gone"))
                .unwrap()
                .machine_id,
            "studio"
        );
    }

    #[test]
    fn remote_target_keeps_a_pick_and_never_guesses_without_a_match() {
        let computers = [
            computer("studio", &["site", "ghostex"], Some("ghostex")),
            computer("linux", &["notes"], None),
        ];
        let studio = handoff_computer(&computers, Some("studio")).unwrap();
        assert_eq!(
            handoff_project(studio, Some("site")).unwrap().project_id,
            "site"
        );
        // A project id from the other computer is not one of this computer's.
        assert_eq!(
            handoff_project(studio, Some("notes")).unwrap().project_id,
            "ghostex"
        );
        let linux = handoff_computer(&computers, Some("linux")).unwrap();
        assert_eq!(handoff_project(linux, None), None);
    }

    #[test]
    fn advanced_handoff_names_the_project_and_only_a_computer_it_leaves_for() {
        let project = ExportTranscriptProject {
            project_id: "p".to_string(),
            title: "acme-web".to_string(),
            path: None,
            same_repo: false,
        };
        let computer = |is_source| ExportTranscriptComputer {
            machine_id: "m".to_string(),
            label: "studio".to_string(),
            is_source,
            projects: vec![project.clone()],
            preferred_project_id: None,
            missing_repository: None,
        };
        assert_eq!(
            advanced_handoff_label(&computer(true), &project),
            "Handoff to acme-web"
        );
        assert_eq!(
            advanced_handoff_label(&computer(false), &project),
            "Handoff to acme-web on studio"
        );
    }

    #[test]
    fn a_saved_handoff_to_remote_comes_back_as_advanced_handoff() {
        let dir = std::env::temp_dir().join(format!(
            "ghostex-advanced-handoff-prefs-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        std::fs::write(&path, r#"{"mode":"handoffRemote"}"#).unwrap();
        assert_eq!(
            load_export_transcript_modal_prefs(&path).mode,
            Some(ExportTranscriptMode::HandoffAdvanced)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn handoff_remote_mode_round_trips_through_the_prefs_file() {
        let dir = std::env::temp_dir().join(format!("gx-export-prefs-{}", std::process::id()));
        let path = dir.join("prefs.json");
        persist_export_transcript_modal_prefs(
            &path,
            ExportTranscriptModalPrefs {
                mode: Some(ExportTranscriptMode::HandoffAdvanced),
                include: ExportTranscriptIncludeOptions::default(),
            },
        );
        let loaded = load_export_transcript_modal_prefs(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded.mode, Some(ExportTranscriptMode::HandoffAdvanced));
    }

    #[test]
    fn handoff_target_of_a_missing_family_is_none() {
        let agents = [agent("claude", Some("claude")), agent("pi", None)];
        assert_eq!(handoff_target_agent(&agents, "codex"), None);
    }
}

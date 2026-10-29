//! A session card's second line in the two-column layout: what the session is doing (when that is
//! worth a word), its agent, and the branch it works on. Cards whose links line or Work mode offer
//! already take the second line keep that instead.

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Hsla, IntoElement, ParentElement, Styled, div, px, rgb};
use gpui_component::h_flex;
use serde_json::Value;

use super::super::appearance::SidebarAppearance;
use super::super::link_chips::LINK_LINE_HEIGHT;
use super::super::model::NativeSidebarSession;
use super::super::status::{FAILED_COLOR, QUESTION_COLOR, WORKING_COLOR};
use super::nav::UNSEEN_COLOR;
use crate::GhostexGpuiApp;

/// The agent's name as the card shows it: `claude` reads Claude.
fn agent_label(name: &str) -> String {
    match name {
        "claude" | "claude-code" => "Claude".to_owned(),
        "codex" => "Codex".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        "gemini" => "Gemini".to_owned(),
        _ => {
            let mut chars = name.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn render_two_column_card_meta(
        &self,
        session: &NativeSidebarSession,
        indent: f32,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        if !self.native_sidebar.two_column.active || self.native_session_has_link_line(session) {
            return None;
        }
        let scale = appearance.scale;
        let facts = super::buckets::facts(session, true);
        let status: Option<(&str, Hsla)> =
            if session.model_selection_failed || session.program_status_failed {
                Some(("Stopped with an error", rgb(FAILED_COLOR).into()))
            } else if facts.waits_on_answer() {
                Some(("Needs your answer", rgb(QUESTION_COLOR).into()))
            } else if facts.is_running() {
                Some(("Working", rgb(WORKING_COLOR).into()))
            } else if session.activity == "attention" {
                Some(("Done", rgb(UNSEEN_COLOR).into()))
            } else if session.lifecycle_state.as_deref() == Some("sleeping") {
                Some(("Sleeping", appearance.muted))
            } else {
                None
            };
        let agent = if super::super::threads::is_coordinator(session) {
            Some("Orchestrator".to_owned())
        } else {
            session
                .details
                .get("agentName")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(agent_label)
        };
        let branch = session
            .details
            .get("branch")
            .and_then(Value::as_str)
            .filter(|branch| !branch.is_empty())
            .map(str::to_owned);
        let separator = || {
            div()
                .flex_shrink_0()
                .text_color(appearance.muted.opacity(0.6))
                .child("·")
        };
        let has_status = status.is_some();
        let has_agent = agent.is_some();
        Some(
            h_flex()
                .h(px(LINK_LINE_HEIGHT * scale))
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                // Under the title, as the links line sits.
                .pl(px((indent + 15.0 + 6.0) * scale))
                .pr(px(6.0 * scale))
                .gap(px(5.0 * scale))
                .text_size(px(12.0 * scale))
                .text_color(appearance.muted)
                .when_some(status, |line, (word, color)| {
                    line.child(div().flex_shrink_0().text_color(color).child(word))
                })
                .when_some(agent, |line, agent| {
                    line.when(has_status, |line| line.child(separator()))
                        .child(div().flex_shrink_0().child(agent))
                })
                .when_some(branch, |line, branch| {
                    line.when(has_status || has_agent, |line| line.child(separator()))
                        .child(
                            gpui::svg()
                                .path("titlebar/git-branch.svg")
                                .size(px(11.0 * scale))
                                .flex_shrink_0()
                                .text_color(appearance.muted),
                        )
                        .child(div().min_w_0().truncate().child(branch))
                })
                .into_any_element(),
        )
    }
}

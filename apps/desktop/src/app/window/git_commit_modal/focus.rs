//! Keyboard focus in the commit review: every control the React dialog makes tabbable is a gpui
//! tab stop in the same order (the order it is painted in), Enter and Space act on the focused
//! control the way they act on a focused button or checkbox, and rings show only after keyboard
//! input (`:focus-visible`, gpui's `last_input_was_keyboard`).
use super::super::native_modal_kit::*;
use super::model::ChangeSide;
use gpui::{App, BoxShadow, Div, FocusHandle, Styled as _, Window, div, point, px, rgb};
use std::collections::HashMap;

/// The dialog's fixed controls, in tab order after the file tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FocusTarget {
    ShowAll,
    DeleteAfter,
    ViewMode,
    LineWrap,
    Whitespace,
    Agent,
    Cancel,
    Merge,
    NewBranch,
    Multiple,
    Confirm,
}

const TARGETS: [FocusTarget; 11] = [
    FocusTarget::ShowAll,
    FocusTarget::DeleteAfter,
    FocusTarget::ViewMode,
    FocusTarget::LineWrap,
    FocusTarget::Whitespace,
    FocusTarget::Agent,
    FocusTarget::Cancel,
    FocusTarget::Merge,
    FocusTarget::NewBranch,
    FocusTarget::Multiple,
    FocusTarget::Confirm,
];

/// A focusable row of the file rail: a section header, a folder (by `ReviewRow::key`), or one
/// side of a file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowFocus {
    Section(ChangeSide),
    Directory(String),
    Open(ChangeSide, String),
}

pub(crate) struct CommitFocus {
    targets: HashMap<FocusTarget, FocusHandle>,
    rows: HashMap<RowFocus, FocusHandle>,
}

impl CommitFocus {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            targets: TARGETS
                .into_iter()
                .map(|target| (target, cx.focus_handle().tab_stop(true)))
                .collect(),
            rows: HashMap::new(),
        }
    }

    pub(crate) fn target(&self, target: FocusTarget) -> &FocusHandle {
        &self.targets[&target]
    }

    /// The row part's handle, made the first time the row is drawn.
    pub(crate) fn row(&mut self, key: RowFocus, cx: &mut App) -> FocusHandle {
        self.rows
            .entry(key)
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    pub(crate) fn focused_target(&self, window: &Window) -> Option<FocusTarget> {
        self.targets
            .iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(target, _)| *target)
    }

    pub(crate) fn focused_row(&self, window: &Window) -> Option<RowFocus> {
        self.rows
            .iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(key, _)| key.clone())
    }

    /// Whether `target` shows its keyboard focus style now.
    pub(crate) fn visible(&self, target: FocusTarget, window: &Window) -> bool {
        self.target(target).is_focused(window) && window.last_input_was_keyboard()
    }
}

/// Whether `handle` shows its keyboard focus style now.
pub(crate) fn focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// shadcn's `--ring` (`oklch(0.556 0 0)`).
const RING: u32 = 0x737373;

/// The 3px `ring-ring/20` halo of a focused shadcn control.
pub(crate) fn focus_ring_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(rgba_of(rgb(RING), 0.2)),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(3.0),
        inset: false,
    }]
}

/// `focus-visible:border-ring` drawn over a control's own border, so focusing never moves layout.
pub(crate) fn focus_ring_border(radius: f32, color: gpui::Rgba) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .rounded(px(radius))
        .border_1()
        .border_color(hsla(color))
}

/// The ring colour of `focus_ring_border` for buttons.
pub(crate) fn ring_color() -> gpui::Rgba {
    rgb(RING)
}

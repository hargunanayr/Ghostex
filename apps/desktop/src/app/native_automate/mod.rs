//! The native GPUI Automate view (the view panel's automations page) and its create/edit dialog.
//! SEE-ALSO: app/helpers/board_gxserver/automation.rs (the bridge logic it calls). It was ported
//! from the deleted React page (apps/desktop/views/project-board/automations.tsx,
//! automation-dialog.tsx and automations-drafts.ts, in git history).
mod actions;
mod detail;
mod dialog;
mod dialog_render;
mod drafts;
mod host;
mod lists;
mod model;
mod render;
mod requests;
mod style;
mod view;

pub(crate) use style::{
    AutomatePalette, CARD_RADIUS, ICON_ALERT, ICON_PLAY, ICON_PLUS, ICON_REFRESH, ICON_TRASH,
    ROW_RADIUS, detail_row, empty_state, group_card, icon, icon_button, secondary_button,
    section_label, status_dot, switch,
};
pub(crate) use view::NativeAutomateView;

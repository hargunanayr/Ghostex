//! The native GPUI Board: every agent thread of the window's Space as a card in Pending, In
//! progress, Review or Completed, with the Space's project filter, Add task, Complete, Reopen,
//! Complete all and Close all, and a card's chat beside the lanes. It covers the whole workarea
//! from the Board button next to Toggle sidebar (`host.rs`). Desktop only, like Automate: the GPUI
//! web build does not compile it. Not the Kanban (`native_kanban/`, the Beads board), whose palette
//! and controls it borrows.
//! SEE-ALSO: packages/gx-core/src/session_board/ (the columns, the Space's threads, the actions),
//! server/src/session_board/ (the Completed flag and Board tasks), gx_store/session_board.rs.
mod card;
mod chat_panel;
mod host;
mod panel;
mod picker;
mod view;

pub(crate) use host::NativeSessionBoardHost;

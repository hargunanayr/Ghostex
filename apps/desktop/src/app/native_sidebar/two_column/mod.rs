//! The two-column sidebar layout (`sidebarLayout` = "twoColumns"): a navigation column with the
//! Needs you / Running / All sessions views and the projects, beside a session column with the
//! selected project's sessions under status and time headings. The rules (which heading, what
//! needs you) are gx-core's `status_buckets.rs`; the session cards are the classic ones
//! (`sessions.rs`, drawn through `session_list.rs`).

mod buckets;
mod card_meta;
mod header;
mod layout;
mod nav;
mod session_column;
mod sprite;
mod state;

pub(crate) use layout::two_column_enabled;
pub(crate) use state::{TwoColumnState, TwoColumnView};

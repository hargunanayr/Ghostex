//! The Unified sidebar's renderer: the header (filter, Add project, New session), the session
//! cards and the folded shelves. The list itself is built by gx-core (`unified_sidebar/`) and
//! installed by the store (gx_store/unified_sidebar.rs).

mod cards;
mod groups;
mod header;
mod hover;
mod list;
pub(crate) mod model;
pub(crate) mod search;
mod shelves;

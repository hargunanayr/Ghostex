//! The Board's server half: the Completed flag and Board tasks. The routes are
//! `server/route_http/session_board.rs`; the columns are worked out in gx-core `session_board/`.

mod completion;
mod pending_task;

pub(crate) use completion::*;
pub(crate) use pending_task::*;

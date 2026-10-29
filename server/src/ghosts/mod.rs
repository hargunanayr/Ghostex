//! Ghosts: persistent agents with a Soul and a Memory, which hold several projects, open threads
//! in them, keep their own skills and documents, and run habits. A Ghost's chat is a coordinator
//! (server/src/coordinators/), so threads and their reports are the Orchestrator's machinery.

mod artifacts;
mod create;
mod endpoint;
mod files;
mod habits;
mod memory;
mod published;
mod records;
mod skills;

pub use artifacts::*;
pub use create::*;
pub use endpoint::*;
pub use files::*;
pub use habits::*;
pub use memory::*;
pub use published::*;
pub use records::*;
pub use skills::*;

/// The manual a Ghost reads with `ghostex ghost guide`.
pub const GHOST_GUIDE: &str = include_str!("guide.md");

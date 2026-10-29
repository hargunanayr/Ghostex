//! The Board: every agent thread of the window's Space as a card in Pending, In progress, Review
//! or Completed (`columns`), the Space's threads (`scope`), the lanes and the project filter
//! (`view`), and what its buttons send (`actions`).
//!
//! SEE-ALSO: server/src/session_board/ (the Completed flag and Board tasks),
//! apps/desktop/src/app/native_session_board/ (the renderer).

mod actions;
mod columns;
mod scope;
mod view;

pub use actions::{
    board_drop, close_completed_message, complete_all_params, complete_all_targets,
    complete_params, read_task_params, start_task_params, task_title_from_prompt,
    update_task_params, with_board_task, BoardDrop, COMPLETE_SESSIONS_ENDPOINT,
    COMPLETE_SESSION_ENDPOINT, PENDING_TASK_PROMPT_PARAM, READ_SESSION_TASK_ENDPOINT,
    START_SESSION_TASK_ENDPOINT, UPDATE_SESSION_TASK_ENDPOINT,
};
pub use columns::{board_column, BoardColumn};
pub use scope::BoardSpace;
pub use view::{
    build_session_board, BoardCard, BoardCardStatus, BoardFilter, BoardLane, BoardProject,
    SessionBoardView,
};

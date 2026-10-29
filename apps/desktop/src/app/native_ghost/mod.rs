//! A Ghost on the desktop: its Overview page in the view panel (threads, documents, habits) and
//! the New Ghost and Ghost settings modal. Desktop only, like the Bot feed: the GPUI web build
//! answers the sidebar's Ghost buttons with stand-ins (apps/gpui-web/src/app/web_host/ghosts.rs).
//! SEE-ALSO: server/src/ghosts/ (every Ghost RPC), app/native_sidebar/ghosts.rs (the sidebar
//! section), app/window/ghost_modal.rs (the modal window).
mod host;
mod modal_host;
mod view;

pub(crate) use view::NativeGhostOverviewView;

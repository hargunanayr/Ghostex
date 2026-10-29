use std::collections::HashSet;

use ghostex_gx_core::{SessionBucket, StatusFilter};

/// What the session column shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TwoColumnView {
    /// The selected project's sessions, under its filter bar.
    #[default]
    Project,
    /// Every project's sessions that match, listed by project.
    Smart(StatusFilter),
}

#[derive(Default)]
pub(crate) struct TwoColumnState {
    /// This frame draws the two-column layout, so session cards carry their second line
    /// (card_meta.rs). Set at the top of every sidebar render.
    pub(crate) active: bool,
    pub(crate) view: TwoColumnView,
    /// The project view's filter bar.
    pub(crate) filter: StatusFilter,
    /// The project the user picked in the navigation column; `None` follows the active project.
    pub(crate) selected_group: Option<String>,
    /// The active project last frame: when the focus moves to another project the column follows
    /// it again.
    pub(crate) followed_active_group: Option<String>,
    /// Headings the user opened or folded against their default, by project.
    pub(crate) toggled_buckets: HashSet<(String, SessionBucket)>,
    pub(crate) nav_scroll: gpui::ScrollHandle,
}

impl TwoColumnState {
    pub(crate) fn bucket_collapsed(&self, group_id: &str, bucket: SessionBucket) -> bool {
        bucket.starts_collapsed()
            != self
                .toggled_buckets
                .contains(&(group_id.to_owned(), bucket))
    }

    pub(crate) fn toggle_bucket(&mut self, group_id: &str, bucket: SessionBucket) {
        let key = (group_id.to_owned(), bucket);
        if !self.toggled_buckets.remove(&key) {
            self.toggled_buckets.insert(key);
        }
    }
}

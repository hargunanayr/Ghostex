//! What the Unified sidebar keeps across a restart.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::build::UnifiedFilter;

/// The client-storage key, in the same table as the Project sidebar's folds and hide list. The web
/// build keeps it in the page's own storage under the same name.
pub const UNIFIED_SAVED_STATE_KEY: &str = "ghostex.sidebar.unified.v1";

/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q855, "remember both"): the header's filter, which shelves are open and which project groups are folded all survive a restart, because a filter you set for your day should still be there after an update. It lives with the Project sidebar's folds, on this client, not as a setting: two computers looking at the same machines can each keep their own view. What is typed in the search field is deliberately not kept: an old search silently hiding sessions after a restart reads as lost work. Shelves are kept by name, and inside a project as `<repo key>|<shelf>`, so a list with grouping off and on each have their own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UnifiedSavedState {
    pub repos: BTreeSet<String>,
    pub machines: BTreeSet<String>,
    pub spaces: BTreeSet<String>,
    /// Shelves toggled away from how they start: Drafts starts open and the rest folded.
    pub toggled_shelves: BTreeSet<String>,
    /// Folded project groups, by repository.
    pub folded_groups: BTreeSet<String>,
    /// The chevron of a card's hover strip is open, on every card.
    pub strip_expanded: bool,
    /// The machine a new session last used per repository (the tray's machine chips).
    pub last_machines: BTreeMap<String, String>,
}

impl UnifiedSavedState {
    /// `None` reads as nothing saved; an unreadable or old value too, since none of this is worth
    /// refusing to start over.
    pub fn parse(raw: Option<&str>) -> Self {
        raw.and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// The filter picks as the builder reads them. The search field starts empty.
    pub fn filter(&self) -> UnifiedFilter {
        UnifiedFilter {
            repos: self.repos.clone(),
            machines: self.machines.clone(),
            spaces: self.spaces.clone(),
            query: String::new(),
        }
    }

    pub fn set_filter(&mut self, filter: &UnifiedFilter) {
        self.repos.clone_from(&filter.repos);
        self.machines.clone_from(&filter.machines);
        self.spaces.clone_from(&filter.spaces);
    }
}

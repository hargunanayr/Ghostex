//! The threads the Board covers: the selected machine's rows in the window's selected Space.
//!
//! CDXC:SessionBoard 2026-10-28 DECISION:
//! User (Ryan): the Board "berlaku masing-masing space di Ghostex (tidak campur space)", with an All projects filter or one project, like PandaCode's. The Board shows the Space the sidebar has selected on the machine the sidebar shows (every project when Spaces are off), never two Spaces at once, and its project filter lists only that Space's projects.
//!
//! The set is the Space's, not the drawn list's, so it is asked of a list built with the Space, the
//! tag filters and the Bots mode lifted (as `plan_space_sleep` asks it), then cut to the selected
//! Space here. Show Hidden is the user's own: a hidden project or thread stays off the Board.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ghostex_gx_protocol::PresentationSession;

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::machine_spaces::section_spaces_enabled;
use crate::sidebar_view::spaces::{
    resolve_selected_space, selection_shows_project, SpaceSelection, SpacesState,
    OTHER_SPACE_ICON, OTHER_SPACE_ID, OTHER_SPACE_LABEL,
};
use crate::sidebar_view::workspaces::window_spaces;
use crate::sidebar_view::{
    SessionRow, SidebarInputs, SidebarMode, SidebarViewModel, WorktreeView, LOCAL_MACHINE_ID,
};

/// One thread with what the Board reads from both the sidebar row and the daemon's row.
#[derive(Clone, Debug)]
pub(crate) struct ScopedThread {
    pub(crate) row: Arc<SessionRow>,
    pub(crate) session: PresentationSession,
    pub(crate) project_id: String,
}

/// One Space the Board can switch to, in the sidebar's order with Other last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardSpace {
    pub space_id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BoardScope {
    /// The selected Space's name; `None` when Spaces are off on the machine.
    pub(crate) space_name: Option<String>,
    /// The machine's Spaces; empty when Spaces are off on it.
    pub(crate) spaces: Vec<BoardSpace>,
    /// The Space's projects in sidebar order, with their titles.
    pub(crate) projects: Vec<(String, String)>,
    /// Each project's folder, for the project pickers' search.
    pub(crate) project_paths: BTreeMap<String, String>,
    /// The collection (the sidebar's coloured folder) each grouped project sits in, by title.
    pub(crate) project_groups: BTreeMap<String, String>,
    /// The worktree each worktree project is, by name and branch.
    pub(crate) project_worktrees: BTreeMap<String, WorktreeView>,
    pub(crate) threads: Vec<ScopedThread>,
}

fn machine_of(machine_id: &str) -> MachineId {
    if machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    }
}

/// The list the Board reads: every group, whatever the sidebar's Space, tag filter or Bots mode.
fn lifted(inputs: &SidebarInputs) -> SidebarInputs {
    let mut probe = inputs.clone();
    probe.ui.collapse.sidebar_mode = SidebarMode::Projects;
    probe.ui.selected_tag_filters.clear();
    probe.spaces_lifted = true;
    probe
}

pub(crate) fn board_scope(core: &Core, inputs: &SidebarInputs, now_ms: u64) -> BoardScope {
    let machine = machine_of(&inputs.ui.selected_machine_id);
    let spaces = section_spaces_enabled(core.presentation(), inputs)
        .then(|| window_spaces(core.presentation(), inputs, &machine))
        .flatten()
        .map(|wire| SpacesState::from_wire(&wire));
    let selection = spaces.as_ref().map(|spaces| {
        resolve_selected_space(
            spaces,
            inputs
                .ui
                .collapse
                .selected_space_by_section
                .get(&inputs.ui.section_key())
                .map(String::as_str),
        )
    });
    let space_name = match (&selection, &spaces) {
        (Some(SpaceSelection::Space(space_id)), Some(spaces)) => spaces
            .spaces
            .get(space_id)
            .map(|space| space.name.clone()),
        (Some(SpaceSelection::Other), _) => Some(OTHER_SPACE_LABEL.to_string()),
        _ => None,
    };
    let board_spaces = match (&selection, &spaces) {
        (Some(selection), Some(spaces)) => spaces
            .ordered()
            .into_iter()
            .map(|space| BoardSpace {
                space_id: space.space_id.clone(),
                name: space.name.clone(),
                icon: space.icon.clone(),
                color: space.color.clone(),
                selected: false,
            })
            .chain(std::iter::once(BoardSpace {
                space_id: OTHER_SPACE_ID.to_string(),
                name: OTHER_SPACE_LABEL.to_string(),
                icon: OTHER_SPACE_ICON.to_string(),
                color: String::new(),
                selected: false,
            }))
            .map(|space| BoardSpace {
                selected: space.space_id == selection.space_id(),
                ..space
            })
            .collect(),
        _ => Vec::new(),
    };

    let built = SidebarViewModel::build_from_scratch(core, &lifted(inputs), now_ms);
    let Some(store) = core.presentation().machine(&machine) else {
        return BoardScope {
            space_name,
            spaces: board_spaces,
            ..BoardScope::default()
        };
    };
    let collection_titles: BTreeMap<&str, &str> = built
        .collections
        .iter()
        .map(|collection| (collection.collection_id.as_str(), collection.title.as_str()))
        .collect();
    let mut project_groups: BTreeMap<String, String> = BTreeMap::new();
    let mut project_worktrees: BTreeMap<String, WorktreeView> = BTreeMap::new();
    let mut projects: Vec<(String, String)> = Vec::new();
    let mut project_ids: BTreeSet<String> = BTreeSet::new();
    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut threads = Vec::new();
    for group in &built.groups {
        if let (Some(selection), Some(spaces)) = (&selection, &spaces) {
            let project = group.space_project.as_ref();
            if !selection_shows_project(
                selection,
                spaces,
                project.map(|project| project.project_id.as_str()),
                project.and_then(|project| project.collection_id.as_deref()),
                project.and_then(|project| project.parent_project_id.as_deref()),
            ) {
                continue;
            }
        }
        if let Some((project, title)) = group.space_project.as_ref().and_then(|project| {
            let collection_id = project.collection_id.as_deref()?;
            Some((project, collection_titles.get(collection_id)?))
        }) {
            project_groups.insert(project.project_id.clone(), title.to_string());
        }
        if let Some(context) = group.core.project_context.as_ref() {
            if let Some(worktree) = context.worktree.clone() {
                project_worktrees.insert(context.project_id.clone(), worktree);
            }
        }
        // A project group names its project; a user-made session group borrows its rows'.
        if let Some(project) = ProjectKey::parse_sidebar_group_id(&group.core.group_id) {
            titles.insert(project.project_id.clone(), group.core.title.clone());
            if project_ids.insert(project.project_id.clone()) {
                projects.push((project.project_id, group.core.title.clone()));
            }
        }
        for view in &group.core.sessions {
            let row = &view.row;
            let Some(key) = row.key.as_ref() else {
                continue;
            };
            if row.is_browser || !seen.insert(row.sidebar_session_id.clone()) {
                continue;
            }
            let Some(session) = store.effective_session(&key.project_id, &key.session_id) else {
                continue;
            };
            threads.push(ScopedThread {
                row: row.clone(),
                session: session.into_owned(),
                project_id: key.project_id.clone(),
            });
        }
    }
    // A thread seen only in a user-made group still belongs to a project of the Space.
    for thread in &threads {
        if project_ids.insert(thread.project_id.clone()) {
            let title = titles
                .get(&thread.project_id)
                .cloned()
                .unwrap_or_else(|| thread.project_id.clone());
            projects.push((thread.project_id.clone(), title));
        }
    }
    let project_paths = store
        .loaded()
        .map(|loaded| {
            projects
                .iter()
                .filter_map(|(project_id, _)| {
                    let path = loaded.project(project_id)?.path.clone()?;
                    Some((project_id.clone(), path))
                })
                .collect()
        })
        .unwrap_or_default();
    BoardScope {
        space_name,
        spaces: board_spaces,
        projects,
        project_paths,
        project_groups,
        project_worktrees,
        threads,
    }
}

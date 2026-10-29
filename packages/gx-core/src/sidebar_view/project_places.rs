//! Where the sidebar puts each project of this computer: its project group (collection) and its
//! place in the list, whatever Space, tag filter or Bots mode the sidebar is showing right now.

use crate::core::Core;
use crate::keys::ProjectKey;

use super::inputs::{SidebarInputs, SidebarMode, LOCAL_MACHINE_ID};
use super::model::SidebarViewModel;
use super::reveal::unfiltered;
use super::view::OrderKind;

/// One local project's place in the sidebar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectPlace {
    pub project_id: String,
    /// The project group's title; empty when the project sits in none.
    pub group: String,
    /// The project group's colour (hex); empty when the project sits in none.
    pub group_color: String,
    /// The top-level row the project is drawn under, in sidebar order.
    pub group_rank: usize,
    /// The project's own place in the whole list.
    pub rank: usize,
}

/// Every project this computer's sidebar lists, in sidebar order. Asked of a list built with the
/// Space, the tag filters, Show Hidden and the Bots mode lifted, so a picker that offers every
/// project still knows the group of one the sidebar is not drawing.
pub fn project_places(core: &Core, inputs: &SidebarInputs, now_ms: u64) -> Vec<ProjectPlace> {
    let mut probe = unfiltered(inputs, SidebarMode::Projects);
    probe.ui.selected_machine_id = LOCAL_MACHINE_ID.to_string();
    let built = SidebarViewModel::build_from_scratch(core, &probe, now_ms);
    let mut places = Vec::new();
    for (group_rank, item) in built.order.iter().enumerate() {
        let (group_ids, group, group_color) = match item.kind {
            OrderKind::Project => (vec![item.id.clone()], String::new(), String::new()),
            OrderKind::Collection => {
                let Some(collection) = built
                    .collections
                    .iter()
                    .find(|collection| collection.collection_id == item.id)
                else {
                    continue;
                };
                (
                    collection.group_ids.clone(),
                    collection.title.clone(),
                    collection.color.clone(),
                )
            }
        };
        for group_id in group_ids {
            let Some(project) = ProjectKey::parse_sidebar_group_id(&group_id) else {
                continue;
            };
            if !project.machine.is_local() {
                continue;
            }
            places.push(ProjectPlace {
                project_id: project.project_id,
                group: group.clone(),
                group_color: group_color.clone(),
                group_rank,
                rank: places.len() + 1,
            });
        }
    }
    places
}

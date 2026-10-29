use std::collections::HashMap;
use std::sync::Arc;

use gpui::{AnyElement, Hsla, IntoElement, ParentElement, Styled, px};
use gpui_component::h_flex;
use serde_json::Value;

use super::super::appearance::SidebarAppearance;
use super::super::model::{NativeSidebarGroup, NativeSidebarSession, NativeSidebarSnapshot};
use super::super::session_group_tree::SessionGroupTree;
use super::buckets::{DayBounds, Tally};
use crate::GhostexGpuiApp;

/// The navigation column's width with labels, before the sidebar zoom.
const NAV_WIDTH: f32 = 180.0;
/// The navigation column as an icon rail, when the sidebar is too narrow for both columns.
const NAV_RAIL_WIDTH: f32 = 46.0;
/// The narrowest the session column may get before the navigation column turns into its rail.
const SESSION_COLUMN_MIN_WIDTH: f32 = 230.0;

/// Whether the list draws the two-column layout. The Hermes bots list keeps its own list.
pub(crate) fn two_column_enabled(snapshot: &NativeSidebarSnapshot) -> bool {
    !snapshot.bots_mode
        && snapshot.hud["settings"][ghostex_gx_core::SIDEBAR_LAYOUT_SETTING_KEY]
            == ghostex_gx_core::SIDEBAR_LAYOUT_TWO_COLUMNS
}

/// One card of the project column: a project group with its projects, or a run of projects
/// outside any group. Projects are indexes into the snapshot's groups, with their depth (a user-made
/// session group sits one level under its project).
pub(super) struct NavBlock {
    pub(super) collection: Option<usize>,
    pub(super) projects: Vec<(usize, u8)>,
}

/// What both columns read, worked out once per frame.
pub(super) struct TwoColumnFrame {
    pub(super) blocks: Vec<NavBlock>,
    /// Pinned orchestrators in list order, with the project each belongs to.
    pub(super) pinned_agents: Vec<(usize, Arc<NativeSidebarSession>)>,
    /// The project group each project sits in, for its colour and the header's subtitle.
    pub(super) collection_of: HashMap<String, usize>,
    /// Each project's pixel creature (sprite.rs), none repeated within a card while there are
    /// creatures left.
    pub(super) sprites: HashMap<String, usize>,
    pub(super) tallies: HashMap<String, Tally>,
    pub(super) totals: Tally,
    /// The project the session column shows.
    pub(super) selected_group: Option<usize>,
    pub(super) parking: bool,
    pub(super) day: DayBounds,
    pub(super) rail: bool,
    pub(super) nav_width: f32,
}

impl TwoColumnFrame {
    fn build(
        content: &NativeSidebarSnapshot,
        selected_id: Option<&str>,
        sidebar_width: f32,
        scale: f32,
    ) -> Self {
        let parking = content.hud["settings"]["enableSessionParking"].as_bool() != Some(false);
        let tree = SessionGroupTree::new(content);
        let index_of = |id: &str| content.groups.iter().position(|group| group.group_id == id);
        let with_children = |group: usize, depth: u8| {
            let mut projects = vec![(group, depth)];
            for child in tree.children(&content.groups[group].group_id) {
                if let Some(index) = index_of(&child.group_id) {
                    projects.push((index, depth + 1));
                }
            }
            projects
        };
        let mut blocks: Vec<NavBlock> = Vec::new();
        let mut collection_of = HashMap::new();
        for item in &content.order {
            if item.kind == "collection" {
                let Some(collection) = content
                    .collections
                    .iter()
                    .position(|collection| collection.collection_id == item.id)
                else {
                    continue;
                };
                let mut projects = Vec::new();
                for group_id in &content.collections[collection].group_ids {
                    if let Some(group) = index_of(group_id) {
                        for (project, depth) in with_children(group, 0) {
                            collection_of
                                .insert(content.groups[project].group_id.clone(), collection);
                            projects.push((project, depth));
                        }
                    }
                }
                blocks.push(NavBlock {
                    collection: Some(collection),
                    projects,
                });
            } else if let Some(group) = index_of(&item.id)
                && !tree.nests(&content.groups[group])
            {
                let projects = with_children(group, 0);
                match blocks.last_mut() {
                    Some(block) if block.collection.is_none() => block.projects.extend(projects),
                    _ => blocks.push(NavBlock {
                        collection: None,
                        projects,
                    }),
                }
            }
        }
        let mut sprites = HashMap::new();
        for block in &blocks {
            let mut taken = std::collections::HashSet::new();
            for (group, _) in &block.projects {
                let id = &content.groups[*group].group_id;
                let mut sprite = super::sprite::preferred_sprite(id);
                while taken.len() < super::sprite::SPRITE_COUNT && !taken.insert(sprite) {
                    sprite = (sprite + 1) % super::sprite::SPRITE_COUNT;
                }
                sprites.insert(id.clone(), sprite);
            }
        }
        let pinned_agents = blocks
            .iter()
            .flat_map(|block| block.projects.iter())
            .flat_map(|(group, _)| {
                content.groups[*group]
                    .sessions
                    .iter()
                    .filter(|session| {
                        session.is_pinned && super::super::threads::is_coordinator(session)
                    })
                    .map(|session| (*group, session.clone()))
            })
            .collect();
        let mut tallies = HashMap::new();
        let mut totals = Tally::default();
        for group in &content.groups {
            let tally = Tally::of(group, parking);
            totals.add(tally);
            tallies.insert(group.group_id.clone(), tally);
        }
        let first_project = blocks
            .iter()
            .find_map(|block| block.projects.first().map(|(group, _)| *group));
        let selected_group = selected_id
            .and_then(index_of)
            .or_else(|| content.groups.iter().position(|group| group.is_active))
            .or(first_project);
        let rail = sidebar_width - NAV_WIDTH * scale < SESSION_COLUMN_MIN_WIDTH * scale;
        Self {
            blocks,
            pinned_agents,
            collection_of,
            sprites,
            tallies,
            totals,
            selected_group,
            parking,
            day: DayBounds::now(),
            rail,
            nav_width: if rail { NAV_RAIL_WIDTH } else { NAV_WIDTH } * scale,
        }
    }

    pub(super) fn tally(&self, group_id: &str) -> Tally {
        self.tallies.get(group_id).copied().unwrap_or_default()
    }

    /// Every project in the column's order, collapsed project groups included.
    pub(super) fn projects(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.blocks
            .iter()
            .flat_map(|block| block.projects.iter().copied())
    }

    /// The pixel creature a project without its own icon shows.
    pub(super) fn sprite(&self, group_id: &str) -> usize {
        self.sprites
            .get(group_id)
            .copied()
            .unwrap_or_else(|| super::sprite::preferred_sprite(group_id))
    }

    /// The colour of the project group a project sits in.
    pub(super) fn tint(&self, content: &NativeSidebarSnapshot, group_id: &str) -> Option<Hsla> {
        self.collection_of
            .get(group_id)
            .and_then(|index| collection_color(&content.collections[*index].color))
    }
}

/// A project group's colour, `#rrggbb`.
pub(super) fn collection_color(color: &str) -> Option<Hsla> {
    u32::from_str_radix(color.trim_start_matches('#'), 16)
        .ok()
        .map(|value| gpui::rgb(value).into())
}

impl GhostexGpuiApp {
    /// The list slot of the sidebar in the two-column layout: the project column and the session
    /// column side by side. `selectors` is the snapshot the Spaces row draws, which is the incoming
    /// one while a Space switch fades the list out.
    pub(crate) fn render_native_sidebar_two_column(
        &mut self,
        content: &NativeSidebarSnapshot,
        selectors: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // The column follows the focus to another project, until the user picks one again.
        let active = content
            .groups
            .iter()
            .find(|group| group.is_active)
            .map(|group| group.group_id.clone());
        let state = &mut self.native_sidebar.two_column;
        if state.followed_active_group != active {
            state.followed_active_group = active;
            state.selected_group = None;
        }
        let frame = TwoColumnFrame::build(
            content,
            state.selected_group.as_deref(),
            self.sidebar_width,
            appearance.scale,
        );
        h_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .child(self.render_two_column_nav(content, selectors, &frame, appearance, cx))
            .child(self.render_two_column_sessions(content, &frame, appearance, cx))
            .into_any_element()
    }
}

/// The icon the user set for the project, or its pixel creature in its project group's colour
/// (sprite.rs), at `size` before the zoom.
pub(super) fn project_icon(
    group: &NativeSidebarGroup,
    hud: &Value,
    tint: Option<Hsla>,
    sprite: usize,
    appearance: &SidebarAppearance,
    size: f32,
) -> Option<AnyElement> {
    if hud["settings"]["showProjectIcons"].as_bool() == Some(false) {
        return None;
    }
    let scale = appearance.scale;
    if group.owner_group_id.is_some() {
        return Some(
            gpui::svg()
                .path("titlebar/folder.svg")
                .size(px(size * scale))
                .flex_shrink_0()
                .text_color(appearance.muted)
                .into_any_element(),
        );
    }
    // Only the icon the user picked; a favicon found in the folder gives way to the creature.
    let image = group
        .project_context
        .as_ref()
        .and_then(|project| project.get("iconDataUrl"))
        .and_then(Value::as_str)
        .and_then(super::super::images::sidebar_image);
    Some(match image {
        Some(image) => gpui::img(image)
            .size(px(size * scale))
            .rounded(px(4.0 * scale))
            .flex_shrink_0()
            .into_any_element(),
        None => super::sprite::project_sprite(
            sprite,
            tint.unwrap_or_else(|| super::sprite::fallback_color(&group.group_id)),
            size * scale,
        ),
    })
}

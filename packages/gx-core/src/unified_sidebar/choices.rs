//! Where a new session starts: the project and machine choices of the new chat's tray, and the
//! project the header's New session aims at.

use std::collections::BTreeMap;

use crate::keys::MachineId;

use super::build::{machine_key, UnifiedFilter};
use super::repo::repo_identity;

/// One project of one machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectChoiceInput {
    pub project_id: String,
    pub title: String,
    pub path: String,
    pub git_remote_origin_url: Option<String>,
    /// A worktree's parent checkout, which marks it as a worktree.
    pub worktree_parent_path: Option<String>,
}

/// One machine and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedChoiceMachineInput {
    pub machine: MachineId,
    pub label: String,
    /// A session can start there now. A machine that isn't connected keeps its chip, dimmed.
    pub connected: bool,
    pub projects: Vec<ProjectChoiceInput>,
}

/// One chip of a repository's row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedChoiceMachine {
    /// `local` for this computer.
    pub machine_id: String,
    pub label: String,
    pub connected: bool,
    pub project_id: String,
}

/// One row of the project search: a repository, with a chip per machine that has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedProjectChoice {
    pub repo_key: String,
    pub label: String,
    pub machines: Vec<UnifiedChoiceMachine>,
    /// Which chip the row starts on.
    pub selected: usize,
}

impl UnifiedProjectChoice {
    pub fn selected_machine(&self) -> Option<&UnifiedChoiceMachine> {
        self.machines.get(self.selected)
    }
}

/// The search's rows.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q848, "one row with machine chips"): a repository on several machines is one row with a chip per machine, so a hundred projects stay a hundred rows; left and right arrows change the chip; a row starts on the machine last used for that repository (`last_machines`), else the one the draft is on, else this computer, else the first. On each machine the row points at its main checkout: a worktree stands in only when that machine has nothing else of the repository, since the tray's This folder / New worktree / Existing chips already pick among worktrees. Machines that are not connected keep their chip, dimmed, because a session can't start there until they are.
pub fn unified_project_choices(
    machines: &[UnifiedChoiceMachineInput],
    last_machines: &BTreeMap<String, String>,
    current_machine: &str,
) -> Vec<UnifiedProjectChoice> {
    let mut by_repo: BTreeMap<String, UnifiedProjectChoice> = BTreeMap::new();
    for machine in machines {
        let machine_id = machine_key(&machine.machine);
        // Per repository: the first main checkout, else the first worktree.
        let mut picks: BTreeMap<String, (&ProjectChoiceInput, String)> = BTreeMap::new();
        for project in &machine.projects {
            let repo = repo_identity(
                project.git_remote_origin_url.as_deref(),
                &project.path,
                project.worktree_parent_path.as_deref(),
                &project.title,
            );
            let replaces = picks.get(&repo.key).is_some_and(|(pick, _)| {
                pick.worktree_parent_path.is_some() && project.worktree_parent_path.is_none()
            });
            if replaces || !picks.contains_key(&repo.key) {
                picks.insert(repo.key, (project, repo.label));
            }
        }
        for (repo_key, (project, label)) in picks {
            let choice = by_repo
                .entry(repo_key.clone())
                .or_insert_with(|| UnifiedProjectChoice {
                    repo_key,
                    label,
                    ..UnifiedProjectChoice::default()
                });
            choice.machines.push(UnifiedChoiceMachine {
                machine_id: machine_id.to_string(),
                label: machine.label.clone(),
                connected: machine.connected,
                project_id: project.project_id.clone(),
            });
        }
    }
    let mut choices: Vec<UnifiedProjectChoice> = by_repo.into_values().collect();
    for choice in &mut choices {
        let chip_of = |machine_id: &str| {
            choice
                .machines
                .iter()
                .position(|machine| machine.machine_id == machine_id)
        };
        choice.selected = last_machines
            .get(&choice.repo_key)
            .and_then(|machine_id| chip_of(machine_id))
            .or_else(|| chip_of(current_machine))
            .or_else(|| chip_of(crate::sidebar_view::LOCAL_MACHINE_ID))
            .unwrap_or(0);
    }
    choices.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.repo_key.cmp(&right.repo_key))
    });
    choices
}

/// The rows whose name, or a machine's name, holds every typed word.
pub fn filter_project_choices(
    choices: &[UnifiedProjectChoice],
    query: &str,
) -> Vec<UnifiedProjectChoice> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    choices
        .iter()
        .filter(|choice| {
            let mut haystack = choice.label.to_lowercase();
            for machine in &choice.machines {
                haystack.push('\n');
                haystack.push_str(&machine.label.to_lowercase());
            }
            words.iter().all(|word| haystack.contains(word))
        })
        .cloned()
        .collect()
}

/// A left or right arrow on a row: the next chip, stopping at the ends.
pub fn move_project_choice_machine(choice: &UnifiedProjectChoice, forward: bool) -> usize {
    if forward {
        (choice.selected + 1).min(choice.machines.len().saturating_sub(1))
    } else {
        choice.selected.saturating_sub(1)
    }
}

/// Where the header's New session starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedNewSessionTarget {
    pub machine_id: String,
    pub project_id: String,
}

/// The project the header's New session aims at: `None` starts in the active project.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q847): the New session button no longer moves your active project. It works out a target, and the picker starts the session there directly. With the filter on one repository that is the active project when it is in that repository, else the entry the row would start on; with any other filter it is the active project. The session opens as a draft, and the tray above its message box changes project or machine.
///
/// CDXC:Sidebar 2026-10-31 WHY:
/// User: "Fix the stray-session bug". The button used to activate the filtered project first, and activating a project with no sessions starts its default agent (gx_store_activate_project), so one press could leave a stray session in that project beside the one the picker then started.
pub fn unified_new_session_target(
    filter: &UnifiedFilter,
    choices: &[UnifiedProjectChoice],
    active_project_id: Option<&str>,
) -> Option<UnifiedNewSessionTarget> {
    if filter.repos.len() != 1 {
        return None;
    }
    let repo_key = filter.repos.first()?;
    let choice = choices.iter().find(|choice| &choice.repo_key == repo_key)?;
    let active_in_repo = active_project_id.is_some_and(|active| {
        choice
            .machines
            .iter()
            .any(|machine| machine.project_id == active)
    });
    if active_in_repo {
        return None;
    }
    let machine = choice.selected_machine()?;
    Some(UnifiedNewSessionTarget {
        machine_id: machine.machine_id.clone(),
        project_id: machine.project_id.clone(),
    })
}

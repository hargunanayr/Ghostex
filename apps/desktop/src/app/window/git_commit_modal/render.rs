//! Drawing the commit review: the file rail, the message editor, the delete-after toggle, the
//! inline diff pane, the footer actions, the file menu and the Merge to main confirmation
//! (`.git-commit-*` in packages/core-ui/styles/modals.css, `.changed-files-tree-*` in commands.css).
use super::super::native_modal_kit::*;
use super::diff_view::{
    hover_scrollbar, render_diff_controls, render_diff_placeholder, render_diff_stat,
    render_diff_surface,
};
use super::focus::{
    FocusTarget, RowFocus, focus_ring_border, focus_ring_shadow, focus_visible, ring_color,
};
use super::model::{ChangeSide, GitChangesView, GitFileStat, ReviewRow, ReviewRowKind};
use super::window::{
    CommitConfirmDialog, GitCommitStagingAction, GpuiGitCommitModalWindow, InlineDiffMode,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, Window, div, list, point, px, radians, rgb,
};
use gpui::{App, Focusable as _};
use gpui_component::input::Textarea;
use gpui_component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuAppearance, PopupMenuItem};
use gpui_component::scroll::Scrollbar;
use gpui_component::{FocusTrapElement as _, Sizable as _, Size as ComponentSize, h_flex, v_flex};

const ICON_CHEVRON_RIGHT: &str = "modals/git-commit/chevron-right.svg";
const ICON_FOLDER: &str = "modals/git-commit/folder.svg";
const ICON_FOLDER_OPEN: &str = "modals/git-commit/folder-open.svg";
const ICON_FILE: &str = "modals/git-commit/file.svg";
const ICON_COPY: &str = "modals/git-commit/copy.svg";
const ICON_TICK: &str = "modals/git-commit/checkbox-tick.svg";
const ICON_PLUS: &str = "modals/git-commit/plus.svg";
const ICON_MINUS: &str = "modals/git-commit/minus.svg";
const ICON_UNDO: &str = "modals/git-commit/undo.svg";
const ICON_LIST: &str = "modals/git-commit/list.svg";
const ICON_TREE: &str = "modals/git-commit/list-tree.svg";

/// The status letter's colour (MonoCode's `statusColor`): untracked sky, added and renamed green,
/// deleted and conflicted red, modified amber.
fn status_color(status: &str, dp: &super::diff_view::GitDiffPalette) -> Rgba {
    match status {
        "U" => rgb(0x38bdf8),
        "A" | "R" => dp.stat_addition,
        "D" | "!" => dp.stat_deletion,
        _ => rgb(0xfbbf24),
    }
}

/// `.git-commit-modal-body { padding: 20px 19px 24px 20px; gap: 16px }` and its
/// `minmax(360px, 0.72fr) minmax(600px, 1.28fr)` columns.
const BODY_PADDING_LEFT: f32 = 20.0;
const BODY_PADDING_RIGHT: f32 = 19.0;
const BODY_GAP: f32 = 16.0;
/// The message row is `minmax(150px, 0.28fr)`, which is its 150px floor at this window's height.
const MESSAGE_ROW_HEIGHT: f32 = 150.0;
/// `--sidebar-density-scale` in the modal host.
const DENSITY: f32 = 0.9;

const MERGE_CONFIRM_TITLE: &str = "Merge worktree into main?";

/// The `.git-commit-modal-shadcn` checkbox skin: 16px, 4px radius, raised fill on a hairline,
/// the foreground fill with a surface-coloured tick when checked, and the browser's 2px focus
/// outline while keyboard-focused. gpui-component's `Checkbox` takes its colours from the global
/// theme, so it cannot wear this per-modal skin.
fn git_checkbox(p: &ModalPalette, checked: bool, focused: bool) -> AnyElement {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(16.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(hsla(if checked { p.foreground } else { p.hairline }))
        .bg(hsla(if checked { p.foreground } else { p.raised }))
        .when(focused, |this| {
            this.shadow(vec![gpui::BoxShadow {
                color: hsla(p.focus_border),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(2.0),
                inset: false,
            }])
        })
        .when(checked, |this| {
            this.child(
                modal_icon(ICON_TICK, 16.0, p.solid_surface)
                    .absolute()
                    .top(px(-1.0))
                    .left(px(-1.0)),
            )
        })
        .into_any_element()
}

impl GpuiGitCommitModalWindow {
    /// `color-mix(in srgb, var(--app-muted) 72%, var(--app-foreground) 28%)`: tree chevrons and icons.
    fn tree_icon_color(&self) -> Rgba {
        let p = self.palette;
        let app_muted = if p.light { rgb(0x717171) } else { p.muted };
        css_mix(app_muted, 0.72, p.foreground)
    }

    /// `.git-commit-files-edit-button` / `-show-all-button`; keyboard focus shows the hover fill.
    fn rail_button(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        target: FocusTarget,
        window: &Window,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let (background, border) = if active {
            (rgba_of(p.foreground, 0.12), rgba_of(p.foreground, 0.22))
        } else if self.focus.visible(target, window) {
            (p.raised_hover, p.hairline)
        } else {
            (p.raised, p.hairline)
        };
        div()
            .id(id)
            .track_focus(self.focus.target(target))
            .flex_shrink_0()
            .flex()
            .items_center()
            .h(px(MODAL_CONTROL_HEIGHT))
            .px(px(10.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(border))
            .bg(hsla(background))
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(p.foreground))
            .cursor_pointer()
            .when(!active, |this| {
                this.hover(move |this| this.bg(hsla(p.raised_hover)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_click(this, window, cx);
            }))
            .child(label)
            .into_any_element()
    }

    /// A 20px icon button on a row or section header (MonoCode's `IconAction`): muted until hovered,
    /// and dimmed while a Stage, Unstage or Discard runs.
    fn row_action(
        &self,
        id: SharedString,
        label: &'static str,
        icon: &'static str,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let hover = rgba_of(p.foreground, 0.10);
        let busy = self.busy;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(20.0))
            .rounded(px(4.0))
            .when(busy, |this| this.opacity(0.4))
            .when(!busy, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        on_click(this, window, cx);
                    }))
            })
            .child(modal_icon(icon, 14.0, self.tree_icon_color()))
            .into_any_element()
    }

    fn render_section_row(
        &mut self,
        row: ReviewRow,
        count: usize,
        expanded: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let side = row.side;
        let icon_color = self.tree_icon_color();
        let hover = rgba_of(p.foreground, 0.06);
        let handle = self.focus.row(RowFocus::Section(side), cx);
        let chevron = modal_icon(ICON_CHEVRON_RIGHT, 14.0, icon_color).flex_shrink_0();
        let actions: Vec<AnyElement> = match side {
            ChangeSide::Staged => vec![self.row_action(
                "git-commit-unstage-all".into(),
                "Unstage All Changes",
                ICON_MINUS,
                |this, _window, cx| this.run_staging(GitCommitStagingAction::Unstage, None, cx),
                cx,
            )],
            ChangeSide::Unstaged => vec![
                self.row_action(
                    "git-commit-discard-all".into(),
                    "Discard All Changes",
                    ICON_UNDO,
                    |this, window, cx| this.request_discard(None, window, cx),
                    cx,
                ),
                self.row_action(
                    "git-commit-stage-all".into(),
                    "Stage All Changes",
                    ICON_PLUS,
                    |this, _window, cx| this.run_staging(GitCommitStagingAction::Stage, None, cx),
                    cx,
                ),
            ],
        };
        div()
            .w_full()
            .when(row.section_gap, |this| this.pt(px(8.0)))
            .child(
                h_flex()
                    .id(SharedString::from(format!("git-commit-section:{side:?}")))
                    .track_focus(&handle)
                    .w_full()
                    .h(px(28.0))
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(6.0))
                    .pr(px(7.0 * DENSITY))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .when(focus_visible(&handle, window), |this| this.bg(hsla(hover)))
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.toggle_section(side, cx)
                    }))
                    .child(if expanded {
                        chevron
                            .with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                            .into_any_element()
                    } else {
                        chevron.into_any_element()
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(10.5))
                            .line_height(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.muted))
                            .child(row.name.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_w(px(16.0))
                            .h(px(16.0))
                            .px(px(5.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(hsla(rgba_of(p.foreground, 0.14)))
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(count.to_string()),
                    )
                    .child(div().flex_1())
                    .children(actions),
            )
            .into_any_element()
    }

    fn render_tree_row(
        &mut self,
        index: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().into_any_element();
        };
        let p = self.palette;
        let dp = self.diff_palette;
        let icon_color = self.tree_icon_color();
        let row_color = css_mix(p.foreground, 0.86, p.muted);
        let hover = rgba_of(p.foreground, 0.06);
        let key = row.key();
        let side = row.side;
        let ReviewRow {
            depth,
            name,
            path,
            stat,
            kind,
            ..
        } = row.clone();
        let name_label = |name: String, folder: Option<String>| {
            h_flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .items_baseline()
                .gap(px(6.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(11.0))
                        .line_height(px(15.714))
                        .font_weight(FontWeight::MEDIUM)
                        .child(name),
                )
                .when_some(folder, |this, folder| {
                    this.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(10.5))
                            .line_height(px(15.714))
                            .text_color(hsla(dp.stat_divider))
                            .child(folder),
                    )
                })
        };
        let tree_stat = |stat: GitFileStat| {
            render_diff_stat(&dp, stat, 10.0, 14.286, FontWeight::MEDIUM, 2.0 * DENSITY)
        };
        let mut element = h_flex()
            .id(SharedString::from(format!("git-commit-tree-row:{key}")))
            .w_full()
            .min_h(px(28.0))
            .items_center()
            .gap(px(7.0))
            .rounded(px(6.0))
            .pl(px(8.0 + depth as f32 * 14.0))
            .pr(px(7.0 * DENSITY))
            .text_color(hsla(row_color));
        match kind {
            ReviewRowKind::Section { count, expanded } => {
                return self.render_section_row(row, count, expanded, window, cx);
            }
            ReviewRowKind::Directory { expanded } => {
                let chevron = modal_icon(ICON_CHEVRON_RIGHT, 14.0, icon_color).flex_shrink_0();
                let handle = self.focus.row(RowFocus::Directory(key.clone()), cx);
                element = element
                    .track_focus(&handle)
                    .when(focus_visible(&handle, window), |this| this.bg(hsla(hover)))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener({
                        let key = key.clone();
                        move |this, _: &ClickEvent, _window, cx| this.toggle_directory(&key, cx)
                    }))
                    .child(if expanded {
                        chevron
                            .with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                            .into_any_element()
                    } else {
                        chevron.into_any_element()
                    })
                    .child(
                        modal_icon(
                            if expanded {
                                ICON_FOLDER_OPEN
                            } else {
                                ICON_FOLDER
                            },
                            14.0,
                            icon_color,
                        )
                        .flex_shrink_0(),
                    )
                    .child(name_label(name, None))
                    .when(!stat.is_zero(), |this| this.child(tree_stat(stat)));
                div().w_full().child(element).into_any_element()
            }
            ReviewRowKind::File { status, folder } => {
                let selected = self.inline_mode == InlineDiffMode::File
                    && self
                        .selected_diff
                        .as_ref()
                        .is_some_and(|(s, p)| *s == side && *p == path);
                let open_handle = self.focus.row(RowFocus::Open(side, path.clone()), cx);
                let focused = focus_visible(&open_handle, window);
                // MonoCode keeps a row's buttons up while it is hovered, focused or selected.
                let show_actions =
                    selected || focused || self.hovered_row.as_deref() == Some(key.as_str());
                let actions: Vec<AnyElement> = if !show_actions {
                    Vec::new()
                } else {
                    match side {
                        ChangeSide::Staged => vec![self.row_action(
                            format!("git-commit-unstage:{path}").into(),
                            "Unstage Changes",
                            ICON_MINUS,
                            {
                                let path = path.clone();
                                move |this, _window, cx| {
                                    this.run_staging(
                                        GitCommitStagingAction::Unstage,
                                        Some(vec![path.clone()]),
                                        cx,
                                    )
                                }
                            },
                            cx,
                        )],
                        ChangeSide::Unstaged => vec![
                            self.row_action(
                                format!("git-commit-discard:{path}").into(),
                                "Discard Changes",
                                ICON_UNDO,
                                {
                                    let path = path.clone();
                                    move |this, window, cx| {
                                        this.request_discard(Some(path.clone()), window, cx)
                                    }
                                },
                                cx,
                            ),
                            self.row_action(
                                format!("git-commit-stage:{path}").into(),
                                "Stage Changes",
                                ICON_PLUS,
                                {
                                    let path = path.clone();
                                    move |this, _window, cx| {
                                        this.run_staging(
                                            GitCommitStagingAction::Stage,
                                            Some(vec![path.clone()]),
                                            cx,
                                        )
                                    }
                                },
                                cx,
                            ),
                        ],
                    }
                };
                element = element
                    .when(focused && !selected, |this| this.bg(hsla(hover)))
                    .when(selected, |this| {
                        this.bg(hsla(rgba_of(p.foreground, 0.10)))
                            .text_color(hsla(p.foreground))
                    })
                    .when(!selected, |this| {
                        this.hover(move |this| this.bg(hsla(hover)))
                    })
                    .on_hover(cx.listener({
                        let key = key.clone();
                        move |this, hovered: &bool, _window, cx| {
                            if *hovered {
                                this.set_hovered_row(Some(key.clone()), cx);
                            } else if this.hovered_row.as_deref() == Some(key.as_str()) {
                                this.set_hovered_row(None, cx);
                            }
                        }
                    }))
                    .when(self.view == GitChangesView::Tree, |this| {
                        this.child(div().flex_shrink_0().w(px(14.0)))
                    })
                    .child(modal_icon(ICON_FILE, 14.0, icon_color).flex_shrink_0())
                    .child(
                        div()
                            .id(SharedString::from(format!("git-commit-open:{key}")))
                            .track_focus(&open_handle)
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let path = path.clone();
                                move |this, _: &ClickEvent, _window, cx| {
                                    this.open_inline_file_diff((side, path.clone()), cx)
                                }
                            }))
                            .child(name_label(name, folder)),
                    )
                    .children(actions)
                    .child(tree_stat(stat))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(12.0))
                            .flex()
                            .justify_end()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(11.0))
                            .line_height(px(15.714))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(status_color(&status, &dp)))
                            .child(status),
                    );
                div()
                    .w_full()
                    .child(element.context_menu(self.file_menu(path, cx)))
                    .into_any_element()
            }
        }
    }

    /// The file row's right-click menu (`SidebarContextMenuPortal` + `AppMenuPanel`): Copy Path and
    /// Open File/Folder Location, as a GPUI-Kit popup menu in the `.ghostex-menu-panel` skin
    /// (CDXC:ContextMenus 2026-09-16 in titlebar/popup_menu_builders.rs): 8px panel, 6px rows and
    /// padding, 10px row inset, 13px x 0.9 text, the menu colours, and no shadow.
    fn file_menu(
        &self,
        path: String,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let p = self.palette;
        let entity = cx.weak_entity();
        let (border, hover) = if p.light {
            (modal_rgba(0x000000, 0.12), rgb(0xefefef))
        } else {
            (modal_rgba(0xffffff, 0.12), rgb(0x202020))
        };
        let row = move |icon: &'static str, label: &'static str| {
            move |_: &mut Window, _: &mut App| {
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .font_family(MODAL_UI_FONT)
                    .text_size(px(13.0 * DENSITY))
                    .line_height(px(13.0 * DENSITY * 1.4))
                    .text_color(hsla(p.foreground))
                    .child(modal_icon(icon, 14.0, p.foreground).flex_shrink_0())
                    .child(label)
            }
        };
        move |menu, _window, _cx| {
            let copy = {
                let (entity, path) = (entity.clone(), path.clone());
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    let _ = entity.update(cx, |this, cx| this.copy_file_path(path.clone(), cx));
                }
            };
            let locate = {
                let (entity, path) = (entity.clone(), path.clone());
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    let _ = entity.update(cx, |this, cx| this.open_file_location(path.clone(), cx));
                }
            };
            menu.appearance(PopupMenuAppearance {
                shadow: false,
                panel_radius: px(8.0),
                item_radius: px(6.0),
                padding: px(6.0),
                item_padding_x: px(10.0),
                item_height: px(16.0 + 13.0 * DENSITY * 1.4),
                separator_margin: px(6.0),
                background: hsla(p.solid_surface),
                foreground: hsla(p.foreground),
                border: hsla(border),
                hover: hsla(hover),
            })
            .min_w(px(156.0))
            .item(PopupMenuItem::element(row(ICON_COPY, "Copy Path")).on_click(copy))
            .item(
                PopupMenuItem::element(row(ICON_FOLDER_OPEN, "Open File/Folder Location"))
                    .on_click(locate),
            )
        }
    }

    fn render_files_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let dp = self.diff_palette;
        let files = &self.draft.changed_files;
        let total = files.len();
        let heading =
            v_flex()
                .min_w_0()
                .gap(px(5.0))
                .when_some(self.draft.branch.clone(), |this, branch| {
                    this.child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(branch.unwrap_or_else(|| "(detached HEAD)".to_string())),
                    )
                });
        let header = h_flex()
            .w_full()
            .min_w_0()
            .min_h(px(30.0))
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(heading);
        let panel = v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.panel))
            .child(header);
        if total == 0 {
            return panel
                .child(
                    div()
                        .py(px(4.0))
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(p.muted))
                        .child("No changed files."),
                )
                .into_any_element();
        }
        let commit_stat = self.commit_stat();
        let views = [
            ModalSegmentedItem {
                icon: Some(ICON_LIST),
                label: "List",
            },
            ModalSegmentedItem {
                icon: Some(ICON_TREE),
                label: "Tree",
            },
        ];
        panel
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(240.0))
                    .min_w_0()
                    .py(px(2.0))
                    .child(
                        list(
                            self.tree_list.clone(),
                            cx.processor(|this: &mut Self, index: usize, window, cx| {
                                this.render_tree_row(index, window, cx)
                            }),
                        )
                        .size_full(),
                    )
                    .child(hover_scrollbar(Scrollbar::vertical(&self.tree_list), &dp)),
            )
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(6.0))
                            .child(div().w(px(150.0)).flex_shrink_0().child(
                                modal_segmented_control(
                                    &p,
                                    "git-commit-changes-view",
                                    &views,
                                    match self.view {
                                        GitChangesView::List => 0,
                                        GitChangesView::Tree => 1,
                                    },
                                    |this: &mut Self, index, _window, cx| {
                                        this.set_changes_view(
                                            if index == 1 {
                                                GitChangesView::Tree
                                            } else {
                                                GitChangesView::List
                                            },
                                            cx,
                                        )
                                    },
                                    cx,
                                ),
                            ))
                            .child(self.rail_button(
                                "git-commit-show-all",
                                "Show All",
                                self.inline_mode == InlineDiffMode::All,
                                FocusTarget::ShowAll,
                                window,
                                |this, _window, cx| this.show_all_file_diffs(cx),
                                cx,
                            )),
                    )
                    .child(render_diff_stat(
                        &dp,
                        commit_stat,
                        12.0,
                        17.14,
                        FontWeight::MEDIUM,
                        4.0,
                    )),
            )
            .into_any_element()
    }

    fn render_message_field(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .h(px(MESSAGE_ROW_HEIGHT))
            .flex_shrink_0()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.14))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.muted))
                    .child("Commit Message"),
            )
            .child(self.render_message_editor(window, cx))
            .into_any_element()
    }

    /// `.git-commit-modal-textarea`: the shell's textarea skin (raised fill, hairline, white edge
    /// while focused, 12px padding) at 14px with a 21px line, filling the message row.
    ///
    /// CDXC:Git 2026-09-28 WHY:
    /// gpui-component's editor wraps 10px short of its own width and draws 20px lines, so the
    /// kit's `modal_text_area` wrapped the suggested body a word early and set it tighter than the
    /// React editor. The frame keeps 2px on the right (12px minus the editor's margin) and the
    /// editor gets the React line height.
    fn render_message_editor(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focused = self.message.read(cx).focus_handle(cx).is_focused(window);
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .pl(px(12.0))
            .pr(px(2.0))
            .py(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
            .bg(hsla(p.raised))
            .child(
                div().flex_1().min_h_0().w_full().child(
                    Textarea::new(&self.message)
                        .on_paste({
                            let modal = cx.entity().downgrade();
                            move |item, window, cx| {
                                modal
                                    .update(cx, |modal, cx| modal.paste_message(item, window, cx))
                                    .unwrap_or(false)
                            }
                        })
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .h_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(14.0))
                        .line_height(px(21.0))
                        .text_color(hsla(p.foreground)),
                ),
            )
            .into_any_element()
    }

    fn render_delete_toggle(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let copy = match &self.draft.worktree_name {
            Some(name) => format!("Delete worktree project after this action finishes ({name})."),
            None => "Delete worktree project after this action finishes.".to_string(),
        };
        h_flex()
            .id("git-commit-delete-worktree")
            .w_full()
            .flex_shrink_0()
            .items_start()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.panel))
            .track_focus(self.focus.target(FocusTarget::DeleteAfter))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.toggle_delete_worktree_after(cx)
            }))
            .child(git_checkbox(
                &p,
                self.delete_worktree_after,
                self.focus.visible(FocusTarget::DeleteAfter, window),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .line_height(px(18.85))
                    .text_color(hsla(p.foreground))
                    .child(copy),
            )
            .into_any_element()
    }

    fn render_left_column(
        &self,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .gap(px(12.0))
            .child(self.render_files_panel(window, cx))
            .when(self.draft.show_commit_message, |this| {
                this.child(self.render_message_field(window, cx))
            })
            // The grid keeps its last `auto` row, and the gap before it, when the toggle is absent.
            .child(if self.draft.is_worktree {
                self.render_delete_toggle(window, cx)
            } else {
                div().h(px(0.0)).into_any_element()
            })
            .into_any_element()
    }

    fn render_diff_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let dp = self.diff_palette;
        let label = self.active_diff_label();
        let has_label = label.is_some();
        let header =
            h_flex()
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .items_center()
                .gap(px(12.0))
                .px(px(12.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(hsla(p.hairline))
                .when_some(label, |this, label| {
                    this.child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_family(MODAL_MONO_FONT)
                                    .text_size(px(12.0))
                                    .line_height(px(17.14))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(dp.path_text))
                                    .child(label),
                            )
                            .child(render_diff_stat(
                                &dp,
                                self.active_diff_stat(),
                                11.0,
                                15.714,
                                FontWeight(650.0),
                                4.0,
                            )),
                    )
                })
                // Without a file label the controls take the grid's first column, one gap short of
                // the edge.
                .when(!has_label, |this| this.child(div().flex_1()))
                .child(div().when(!has_label, |this| this.mr(px(12.0))).child(
                    render_diff_controls(
                        &dp,
                        self.prefs,
                        [
                            self.focus.target(FocusTarget::ViewMode),
                            self.focus.target(FocusTarget::LineWrap),
                            self.focus.target(FocusTarget::Whitespace),
                        ],
                        window,
                        |this: &mut Self, prefs, _window, cx| this.set_diff_prefs(prefs, cx),
                        cx,
                    ),
                ));
        let body = if self.is_selected_diff_loading() {
            render_diff_placeholder(&dp, "Loading diff...")
        } else if !self.diff.has_patch() {
            render_diff_placeholder(
                &dp,
                if self.draft.changed_files.is_empty() {
                    "No changed files to preview."
                } else {
                    "Select a file to preview its diff."
                },
            )
        } else {
            render_diff_surface(&self.diff, &dp, self.prefs, "git-commit-diff-rows", window)
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.panel))
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().min_h_0().w_full().child(body))
            .into_any_element()
    }

    /// A `.git-commit-modal-button`: the shell's neutral filled pill, sized to its label, at least 132px,
    /// with shadcn's keyboard ring. A disabled button is not a tab stop.
    fn footer_button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        disabled: bool,
        target: FocusTarget,
        window: &Window,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = !disabled && self.focus.visible(target, window);
        h_flex()
            .relative()
            .flex_shrink_0()
            .min_w(px(132.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .when(!disabled, |this| {
                this.track_focus(self.focus.target(target))
            })
            .when(focused, |this| this.shadow(focus_ring_shadow()))
            .child(modal_action_button(
                &self.palette,
                id,
                label,
                None,
                ModalButtonTone::Neutral,
                disabled,
                on_click,
                cx,
            ))
            .when(focused, |this| {
                this.child(focus_ring_border(MODAL_RADIUS_CONTROL, ring_color()))
            })
            .into_any_element()
    }

    fn render_footer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let show_message = self.draft.show_commit_message;
        let can_confirm = self.can_confirm();
        let agent_value = self
            .selected_agent_index()
            .and_then(|index| self.agents.get(index))
            .map(|agent| agent.name.clone());
        h_flex()
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .pt(px(14.0))
            .pb(px(20.0))
            .pl(px(20.0))
            .pr(px(19.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .when(!self.agents.is_empty(), |this| {
                this.child(
                    h_flex()
                        .relative()
                        .w(px(160.0))
                        .flex_shrink_0()
                        .track_focus(self.focus.target(FocusTarget::Agent))
                        .on_children_prepainted(capture_child_bounds(
                            self.agent_select.trigger_bounds.clone(),
                            0,
                        ))
                        .child(modal_select_trigger(
                            &p,
                            &self.agent_select,
                            "git-commit-agent-select",
                            agent_value,
                            "Select agent",
                            false,
                            |this: &mut Self, window, cx| this.toggle_agent_select(window, cx),
                            cx,
                        ))
                        // `.gx-app-modal [data-slot='select-trigger']:focus-visible`: a white edge, no halo.
                        .when(self.focus.visible(FocusTarget::Agent, window), |this| {
                            this.child(focus_ring_border(MODAL_RADIUS_CONTROL, p.focus_border))
                        }),
                )
            })
            .child(div().flex_1())
            .child(self.footer_button(
                "git-commit-cancel",
                "Cancel",
                false,
                FocusTarget::Cancel,
                window,
                |this, window, cx| this.cancel(window, cx),
                cx,
            ))
            .when(self.draft.is_worktree, |this| {
                this.child(self.footer_button(
                    "git-commit-merge",
                    "Merge to main",
                    !self.can_run_direct_merge(),
                    FocusTarget::Merge,
                    window,
                    |this, window, cx| this.open_merge_confirm(window, cx),
                    cx,
                ))
            })
            .when(show_message, |this| {
                this.child(self.footer_button(
                    "git-commit-new-branch",
                    "Commit on new branch",
                    !can_confirm,
                    FocusTarget::NewBranch,
                    window,
                    |this, window, cx| this.confirm(true, window, cx),
                    cx,
                ))
                .child(self.footer_button(
                    "git-commit-multiple",
                    "Multiple Commits",
                    !can_confirm,
                    FocusTarget::Multiple,
                    window,
                    |this, window, cx| this.multiple_commits(window, cx),
                    cx,
                ))
            })
            .child(self.footer_button(
                "git-commit-confirm",
                self.draft.confirm_label.clone(),
                !can_confirm,
                FocusTarget::Confirm,
                window,
                |this, window, cx| this.confirm(false, window, cx),
                cx,
            ))
            .into_any_element()
    }

    /// `ConfirmationModal` for Merge to main: a centered app-modal card over a 50% black backdrop,
    /// trapping Tab between its two buttons the way Radix does.
    fn render_merge_confirm(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.confirm_dialog.clone()?;
        let p = self.palette;
        let (title, description, label): (String, String, &'static str) = match dialog {
            CommitConfirmDialog::Merge => (
                MERGE_CONFIRM_TITLE.to_string(),
                format!(
                    "This will merge {} directly into main without creating a PR.",
                    self.draft
                        .worktree_name
                        .as_deref()
                        .unwrap_or("this worktree")
                ),
                "Merge to main",
            ),
            CommitConfirmDialog::Discard {
                title,
                description,
                label,
                ..
            } => (title, description, label),
        };
        let focus_ring = |button: AnyElement, handle: &gpui::FocusHandle| {
            let focused = focus_visible(handle, window);
            h_flex()
                .relative()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .track_focus(handle)
                .when(focused, |this| this.shadow(focus_ring_shadow()))
                .child(button)
                .when(focused, |this| {
                    this.child(focus_ring_border(MODAL_RADIUS_CONTROL, ring_color()))
                })
                .into_any_element()
        };
        let cancel = modal_action_button(
            &p,
            "git-commit-merge-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, window, cx| this.close_merge_confirm(window, cx),
            cx,
        );
        let confirm = modal_action_button(
            &p,
            "git-commit-merge-confirm",
            label,
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, window, cx| this.accept_confirm_dialog(window, cx),
            cx,
        );
        Some(
            div()
                .id("git-commit-merge-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(modal_rgba(0x000000, 0.5)))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.close_merge_confirm(window, cx)
                }))
                .child(
                    v_flex()
                        .id("git-commit-merge-dialog")
                        .occlude()
                        .w(px(460.0))
                        .p(px(MODAL_WINDOW_PADDING))
                        .gap(px(MODAL_SECTION_GAP))
                        .rounded(px(MODAL_RADIUS_SECTION))
                        .border_1()
                        .border_color(hsla(rgba_of(p.foreground, 0.05)))
                        .bg(hsla(p.solid_surface))
                        .shadow_xl()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(modal_header(&p, title, Some(description)))
                        .child(modal_footer(vec![
                            focus_ring(cancel, &self.merge_cancel_focus),
                            focus_ring(confirm, &self.merge_confirm_focus),
                        ]))
                        .focus_trap("git-commit-merge-trap", &self.merge_trap_focus),
                )
                .into_any_element(),
        )
    }

    fn render_agent_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        modal_select_menu(
            &self.palette,
            &self.agent_select,
            "git-commit-agent-menu",
            &items,
            self.selected_agent_index(),
            |this: &mut Self, index, window, cx| this.choose_agent(index, window, cx),
            |this: &mut Self, window, cx| this.dismiss_agent_select(window, cx),
            window,
            cx,
        )
    }
}

impl Render for GpuiGitCommitModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let viewport = window.viewport_size();
        let columns = f32::from(viewport.width) - BODY_PADDING_LEFT - BODY_PADDING_RIGHT - BODY_GAP;
        let left_width = (columns * 0.72 / 2.0)
            .max(360.0)
            .min(columns - 600.0)
            .max(0.0);
        let agent_menu = self.render_agent_menu(window, cx);
        let merge_confirm = self.render_merge_confirm(window, cx);
        div()
            .id("ghostex-gpui-git-commit-modal")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .capture_action(cx.listener(Self::on_escape_action))
            .capture_action(cx.listener(Self::on_indent_action))
            .capture_action(cx.listener(Self::on_outdent_action))
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .items_start()
                            .gap(px(BODY_GAP))
                            .pt(px(20.0))
                            .pb(px(24.0))
                            .pl(px(BODY_PADDING_LEFT))
                            .pr(px(BODY_PADDING_RIGHT))
                            .child(self.render_left_column(left_width, window, cx))
                            .child(self.render_diff_panel(window, cx)),
                    )
                    .child(self.render_footer(window, cx)),
            )
            .children(agent_menu)
            .children(merge_confirm)
    }
}

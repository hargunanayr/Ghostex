//! Preview host for the native commit review (`git-commit`) and the standalone file diff
//! (`git-file-diff`).
//! `git-commit` states (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): the default is the Storybook review
//! whose first file's diff answers after half a second; `loading` never answers; `nodiff` answers
//! with the "No diff is available" text; `empty` has no changed files; `long` has sixty files in
//! nested folders; `nomessage` is a push review (no message editor, not a worktree). After the
//! first diff, `tree`, `hover`, `discard`, `all`, `confirm`, `agents`, `menu`, `split`, `wrap` and `collapsed` put the
//! dialog in that state. `keyescape` presses Escape; `keymerge` opens the confirmation and presses
//! Tab then Enter; `tabN` presses Tab N times from the message editor.
//! `git-file-diff` states: the default is the Storybook diff; `split`, `wrap` and `long`.
//! Either dialog takes `+`-joined pointer steps after its state, run 600ms after it shows its
//! diff: `drag:x1,y1,x2,y2` drags a selection, `click2:x,y` and `click3:x,y` double and triple
//! click, `wheel:x,y,dy` scrolls, `copy` prints what Ctrl/Cmd+C would copy and `ctrlc` presses it
//! and prints the clipboard, then puts the old clipboard back (e.g.
//! `wrap+drag:150,260,300,330+copy`).
use super::git_commit_modal::*;
use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Keystroke, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiGitCommitModalWindow>)>>>;

fn side(status: &str, additions: u64, deletions: u64) -> Option<GitFileSide> {
    Some(GitFileSide {
        status: status.to_string(),
        additions,
        deletions,
    })
}

/// A staged addition, a partly staged file (in both sections), an unstaged edit, an untracked
/// file and an unstaged deletion.
fn story_files() -> Vec<GitChangedFile> {
    let file = |path: &str, staged, unstaged| GitChangedFile {
        path: path.to_string(),
        additions: 0,
        deletions: 0,
        staged,
        unstaged,
    };
    vec![
        file(
            "packages/core-ui/modal-gallery/app-host-prompts.stories.tsx",
            side("A", 182, 0),
            None,
        ),
        file(
            "packages/core-ui/modal-gallery/overview.stories.tsx",
            side("M", 60, 2),
            side("M", 36, 0),
        ),
        file(
            "apps/desktop/src/app/window/git_commit_modal/render.rs",
            None,
            side("M", 214, 88),
        ),
        file("docs/notes/staging.md", None, side("U", 0, 0)),
        file("packages/shared/legacy-select.ts", None, side("D", 0, 41)),
    ]
}

fn long_files() -> Vec<GitChangedFile> {
    let folders = [
        "apps/desktop/src/app/window",
        "apps/desktop/src/app/gx_store/git",
        "packages/core-ui/styles",
        "packages/shared",
        "server/src/agents",
        "docs",
    ];
    (0..60)
        .map(|index| {
            let change = side(
                if index % 5 == 0 { "A" } else { "M" },
                (index * 7 % 50) as u64,
                (index * 3 % 11) as u64,
            );
            GitChangedFile {
                path: format!(
                    "{}/file-{index}.{}",
                    folders[index % folders.len()],
                    if index % 3 == 0 { "rs" } else { "ts" }
                ),
                additions: (index * 7 % 50) as u64,
                deletions: (index * 3 % 11) as u64,
                staged: if index % 4 == 0 { change.clone() } else { None },
                unstaged: if index % 4 == 0 { None } else { change },
            }
        })
        .collect()
}

fn story_draft(state: &str) -> GitCommitDraft {
    let changed_files = match state {
        "empty" => Vec::new(),
        "long" => long_files(),
        _ => story_files(),
    };
    let push = state == "nomessage";
    GitCommitDraft {
        request_id: "storybook-git-commit".to_string(),
        branch: Some(Some("feat/modal-gallery".to_string())),
        changed_files,
        confirm_label: if push { "Push" } else { "Commit changes" }.to_string(),
        delete_worktree_after_default: false,
        is_worktree: !push,
        show_commit_message: !push,
        suggested_subject: "feat: add modal gallery".to_string(),
        suggested_body: Some(
            "Collect every active React modal under one Storybook navigation root.".to_string(),
        ),
        worktree_name: Some("Ghostex-modal-gallery".to_string()),
    }
}

/// A patch with every row kind and every token colour.
fn fake_patch(path: &str) -> String {
    [
        format!("diff --git a/{path} b/{path}"),
        "index 3b18e51..a9c2f3d 100644".to_string(),
        format!("--- a/{path}"),
        format!("+++ b/{path}"),
        "@@ -12,7 +12,9 @@ export function main() {".to_string(),
        " export const modalStories = [".to_string(),
        "-  'Settings',".to_string(),
        "+  'Settings',".to_string(),
        "+  render(42, true); // a comment".to_string(),
        "   @Component String".to_string(),
        "+\tconst value = await fetchNumber(0x1f) /* inline */ + 3.5;".to_string(),
        "-   ".to_string(),
        "+  # shell style comment".to_string(),
        "   return `template ${value}`;".to_string(),
        " }".to_string(),
        "@@ -40,6 +42,6 @@".to_string(),
        "-const veryLongLineThatKeepsGoing = someFunctionCall(argumentNumberOne, argumentNumberTwo, argumentNumberThree, argumentNumberFour, argumentNumberFive);".to_string(),
        "+const veryLongLineThatKeepsGoing = someFunctionCall(argumentNumberOne, argumentNumberTwo, argumentNumberThree, argumentNumberFour, argumentNumberFive, argumentNumberSix);".to_string(),
    ]
    .join("\n")
}

fn long_patch() -> String {
    let mut lines = vec![
        "diff --git a/src/big.rs b/src/big.rs".to_string(),
        "@@ -1,400 +1,400 @@".to_string(),
    ];
    for index in 0..600 {
        lines.push(match index % 4 {
            0 => format!("+    let value_{index} = compute({index});"),
            1 => format!("-    let value_{index} = old_compute({index});"),
            _ => format!("     // unchanged line {index}"),
        });
    }
    lines.join("\n")
}

fn deliver_diff(slot: &Slot, path: String, staged: bool, state: &str, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        return;
    };
    let patch = if state == "nodiff" {
        format!("No diff is available for {path}.")
    } else {
        fake_patch(&path)
    };
    let draft = GitFileDiffDraft {
        file_path: path,
        patch,
        additions: Some(12),
        deletions: Some(5),
        staged: Some(staged),
    };
    let _ = window.update(cx, |_root, _window, cx| {
        view.update(cx, |modal, cx| modal.receive_file_diff(draft, cx));
    });
}

/// A `+`-joined state: the named states first, then the pointer steps.
fn split_state(state: &str) -> (Vec<String>, Vec<String>) {
    state
        .split('+')
        .map(str::to_string)
        .partition(|part| !is_pointer_step(part))
}

fn is_pointer_step(part: &str) -> bool {
    part == "copy"
        || part == "ctrlc"
        || ["drag:", "click2:", "click3:", "wheel:"]
            .iter()
            .any(|prefix| part.starts_with(prefix))
}

fn numbers(list: &str) -> Vec<f32> {
    list.split(',')
        .filter_map(|n| n.trim().parse().ok())
        .collect()
}

fn mouse_move(
    window: &mut gpui::Window,
    position: gpui::Point<gpui::Pixels>,
    pressed_button: Option<gpui::MouseButton>,
    cx: &mut App,
) {
    window.dispatch_event(
        gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position,
            pressed_button,
            modifiers: gpui::Modifiers::default(),
        }),
        cx,
    );
}

fn mouse_button(
    window: &mut gpui::Window,
    position: gpui::Point<gpui::Pixels>,
    down: bool,
    click_count: usize,
    cx: &mut App,
) {
    let (button, modifiers) = (gpui::MouseButton::Left, gpui::Modifiers::default());
    let input = if down {
        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            button,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        })
    } else {
        gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
            button,
            position,
            modifiers,
            click_count,
        })
    };
    window.dispatch_event(input, cx);
}

/// One pointer step: a drag in eight moves, a double or triple click, a wheel scroll, or a copy
/// printout.
fn pointer_step(step: &str, window: &mut gpui::Window, cx: &mut App) {
    let at = |n: &[f32], i: usize| gpui::point(gpui::px(n[i]), gpui::px(n[i + 1]));
    if let Some(list) = step.strip_prefix("drag:") {
        let n = numbers(list);
        if n.len() != 4 {
            return;
        }
        let (start, end) = (at(&n, 0), at(&n, 2));
        mouse_move(window, start, None, cx);
        mouse_button(window, start, true, 1, cx);
        for index in 1..=8 {
            let t = index as f32 / 8.0;
            let position = gpui::point(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            );
            mouse_move(window, position, Some(gpui::MouseButton::Left), cx);
        }
        mouse_button(window, end, false, 1, cx);
    } else if let Some((clicks, list)) = step
        .strip_prefix("click2:")
        .map(|list| (2, list))
        .or_else(|| step.strip_prefix("click3:").map(|list| (3, list)))
    {
        let n = numbers(list);
        if n.len() != 2 {
            return;
        }
        let position = at(&n, 0);
        mouse_move(window, position, None, cx);
        for click_count in 1..=clicks {
            mouse_button(window, position, true, click_count, cx);
            mouse_button(window, position, false, click_count, cx);
        }
    } else if let Some(list) = step.strip_prefix("wheel:") {
        let n = numbers(list);
        if n.len() != 3 {
            return;
        }
        let position = at(&n, 0);
        mouse_move(window, position, None, cx);
        window.dispatch_event(
            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(n[2]))),
                modifiers: gpui::Modifiers::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }),
            cx,
        );
    } else if step == "ctrlc" {
        let before = cx.read_from_clipboard();
        let chord = if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        };
        window.dispatch_keystroke(Keystroke::parse(chord).expect("a valid chord"), cx);
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        eprintln!("ctrl-c {copied:?}");
        if let Some(before) = before {
            cx.write_to_clipboard(before);
        }
    } else if step == "copy" {
        let text = gpui_component::TextSelection::selected_text(window, cx);
        eprintln!("copy {:?}", text.trim());
    }
    window.refresh();
}

/// Runs the pointer steps 600ms after the dialog drew its diff (wrapped rows need a frame to
/// learn their width), 150ms apart.
fn run_pointer_steps(window: AnyWindowHandle, steps: Vec<String>, cx: &mut App) {
    if steps.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(600))
            .await;
        for step in steps {
            cx.update(|cx| {
                let _ = window.update(cx, |_root, window, cx| pointer_step(&step, window, cx));
            });
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
        }
    })
    .detach();
}

fn apply_state(slot: &Slot, state: &str, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        return;
    };
    let (names, steps) = split_state(state);
    run_pointer_steps(window.into(), steps, cx);
    for name in names.iter().skip(1) {
        let _ = window.update(cx, |_root, window, cx| {
            view.update(cx, |modal, cx| modal.preview_state(name, window, cx));
        });
    }
    let state = names.first().map(String::as_str).unwrap_or_default();
    let _ = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| {
            modal.preview_state(
                if state == "keymerge" {
                    "confirm"
                } else {
                    state
                },
                window,
                cx,
            )
        });
    });
    match state {
        // A right-click on the second file row, where the Storybook capture right-clicks it.
        "menu" => {
            let any_window: AnyWindowHandle = window.into();
            let _ = any_window.update(cx, |_root, window, cx| {
                let position = gpui::point(gpui::px(157.0), gpui::px(144.0));
                window.dispatch_event(
                    gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers: gpui::Modifiers::default(),
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                        button: gpui::MouseButton::Right,
                        position,
                        modifiers: gpui::Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }),
                    cx,
                );
            });
        }
        "keyescape" => dispatch_keys(slot, &["escape"], cx),
        "keymerge" => dispatch_keys(slot, &["tab", "enter"], cx),
        _ => {
            // `tabN`: Tab N times from the message editor, to see where keyboard focus lands;
            // `keys:a,b,c`: those keystrokes in order.
            if let Some(count) = state.strip_prefix("tab").and_then(|n| n.parse().ok()) {
                let keys: &'static [&'static str] =
                    Box::leak(vec!["tab"; count].into_boxed_slice());
                dispatch_keys(slot, keys, cx);
            } else if let Some(list) = state.strip_prefix("keys:") {
                let keys: Vec<&'static str> = list
                    .split(',')
                    .map(|key| &*Box::leak(key.to_string().into_boxed_str()))
                    .collect();
                dispatch_keys(slot, Box::leak(keys.into_boxed_slice()), cx);
            }
        }
    }
}

fn log_confirm(kind: &str, confirm: &GitCommitConfirm) {
    eprintln!("{kind} {confirm:?}");
}

/// Dispatches keystrokes into the demo window 250ms apart, the way the platform would.
fn dispatch_keys(slot: &Slot, keys: &'static [&'static str], cx: &mut App) {
    let slot = slot.clone();
    cx.spawn(async move |cx| {
        for key in keys {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let target = slot.borrow().clone();
            let Some((window, _view)) = target else {
                return;
            };
            let any_window: AnyWindowHandle = window.into();
            cx.update(|cx| {
                let _ = any_window.update(cx, |_root, window, cx| {
                    let keystroke = Keystroke::parse(key).expect("a valid demo keystroke");
                    window.dispatch_keystroke(keystroke, cx);
                });
            });
        }
    })
    .detach();
}

/// The palette the app would pass: tinted by the chrome colour, and frosted when
/// `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1` (a translucent fill stands in for the app's blurred one).
fn demo_palette(demo: &super::DemoEnv) -> super::native_modal_kit::ModalPalette {
    let palette = demo.palette.tinted(demo.palette.surface);
    if super::env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let fill = super::native_modal_kit::rgba_of(palette.surface, 0.72);
        return palette.frosted(fill);
    }
    palette
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let palette = demo_palette(demo);
    let agents = [("codex", "Codex"), ("claude", "Claude Code")]
        .into_iter()
        .map(|(agent_id, name)| GitCommitAgent {
            agent_id: agent_id.to_string(),
            name: name.to_string(),
        })
        .collect();
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let state = demo.state.clone();
    // The named state the dialog opens in; pointer steps run once the first diff is shown.
    let base = split_state(&state).0.into_iter().next().unwrap_or_default();
    let first_diff = Rc::new(std::cell::Cell::new(true));
    let host: GitCommitModalHost = Rc::new(move |command, cx: &mut App| match command {
        GitCommitModalCommand::OpenFileDiff {
            request_id,
            file_path,
            staged,
        } => {
            eprintln!("open file diff {request_id} {file_path} staged={staged}");
            if base == "loading" {
                return;
            }
            let slot = host_slot.clone();
            let state = state.clone();
            let base = base.clone();
            let first = first_diff.replace(false);
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                cx.update(|cx| {
                    deliver_diff(&slot, file_path, staged, &base, cx);
                    if first {
                        apply_state(&slot, &state, cx);
                    }
                });
            })
            .detach();
        }
        GitCommitModalCommand::Confirm(confirm) => {
            log_confirm("confirm", &confirm);
            cx.quit();
        }
        GitCommitModalCommand::DirectMerge(confirm) => {
            log_confirm("direct merge", &confirm);
            cx.quit();
        }
        GitCommitModalCommand::MultipleCommits {
            request_id,
            agent_id,
        } => {
            eprintln!("multiple commits {request_id} {agent_id:?}");
            cx.quit();
        }
        GitCommitModalCommand::Cancel { request_id } => {
            eprintln!("cancel {request_id}");
            cx.quit();
        }
        GitCommitModalCommand::OpenFileLocation {
            request_id,
            file_path,
        } => eprintln!("open location {request_id} {file_path}"),
        GitCommitModalCommand::PromptAgentChanged(agent_id) => {
            eprintln!("prompt agent {agent_id}")
        }
        GitCommitModalCommand::DiffPrefsChanged(prefs) => eprintln!("diff prefs {prefs:?}"),
        GitCommitModalCommand::PathCopied => eprintln!("path copied"),
        GitCommitModalCommand::Staging {
            request_id,
            action,
            file_paths,
        } => eprintln!("staging {request_id} {action:?} {file_paths:?}"),
        GitCommitModalCommand::ChangesViewChanged(view) => eprintln!("changes view {view:?}"),
    });
    let config = GitCommitModalConfig {
        draft: story_draft(
            split_state(&demo.state)
                .0
                .first()
                .map_or("", String::as_str),
        ),
        agents,
        prompt_agent_id: Some("codex".to_string()),
        diff_prefs: GitDiffPrefs::default(),
        changes_view: GitChangesView::default(),
        palette,
    };
    let (window, view) = super::open_large_modal_window(
        GIT_COMMIT_MODAL_WIDTH,
        GIT_COMMIT_MODAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiGitCommitModalWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view));
    if matches!(demo.state.as_str(), "empty") {
        let slot = slot.clone();
        let state = demo.state.clone();
        cx.defer(move |cx| apply_state(&slot, &state, cx));
    }
}

pub(super) fn open_file_diff(demo: &super::DemoEnv, cx: &mut App) {
    let palette = demo_palette(demo);
    let (names, steps) = split_state(&demo.state);
    let patch = if names.iter().any(|name| name == "long") {
        long_patch()
    } else {
        [
            "@@ -12,7 +12,9 @@",
            " export const modalStories = [",
            "-  'Settings',",
            "+  'Settings',",
            "+  'Session Note',",
            "+  'Export Transcript',",
            "   'Add Project',",
        ]
        .join("\n")
    };
    let draft = GitFileDiffDraft {
        file_path: "packages/core-ui/modal-gallery/overview.stories.tsx".to_string(),
        patch,
        additions: Some(12),
        deletions: Some(5),
        staged: None,
    };
    let host: GitFileDiffModalHost = Rc::new(|command, cx: &mut App| match command {
        GitFileDiffModalCommand::Close => {
            eprintln!("close");
            cx.quit();
        }
    });
    let state = names.first().cloned().unwrap_or_default();
    let (window, view) = super::open_large_modal_window(
        GIT_FILE_DIFF_MODAL_WIDTH,
        GIT_FILE_DIFF_MODAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiGitFileDiffModalWindow::new(draft, palette, host, window, cx))
        },
        cx,
    );
    let prefs = match state.as_str() {
        "split" => Some(GitDiffPrefs {
            view_mode: GitDiffViewMode::Split,
            ..GitDiffPrefs::default()
        }),
        "wrap" => Some(GitDiffPrefs {
            line_wrap: true,
            ..GitDiffPrefs::default()
        }),
        _ => None,
    };
    if let Some(prefs) = prefs {
        let _ = window.update(cx, |_root, _window, cx| {
            view.update(cx, |modal, cx| modal.preview_prefs(prefs, cx));
        });
    }
    run_pointer_steps(window.into(), steps, cx);
}

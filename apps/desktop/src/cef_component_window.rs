use super::*;

const CEF_COMPONENT: &str = "cef";
pub(crate) const CEF_RUNTIME_DIR_ENV: &str = "GHOSTEX_CEF_DIR";
#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_ENV: &str = "GHOSTEX_CEF_FRAMEWORK_EXECUTABLE";
const CEF_COMPONENT_WINDOW_WIDTH: f32 = 520.0;
const CEF_COMPONENT_WINDOW_HEIGHT: f32 = 300.0;
#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH: &str =
    "Chromium Embedded Framework.framework/Chromium Embedded Framework";

#[derive(Clone, Debug)]
enum CefComponentWindowState {
    Installing(CefComponentProgress),
    Failed(String),
}

#[derive(Clone, Copy, Debug)]
struct CefComponentProgress {
    downloaded_bytes: u64,
    phase: component_store::ComponentStoreProgressPhase,
    size_bytes: u64,
}

impl CefComponentProgress {
    fn checking() -> Self {
        Self {
            downloaded_bytes: 0,
            phase: component_store::ComponentStoreProgressPhase::Checking,
            size_bytes: 0,
        }
    }
}

pub(crate) struct GpuiCefComponentWindow {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    state: CefComponentWindowState,
    version: String,
}

impl GpuiCefComponentWindow {
    fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        state: CefComponentWindowState,
        version: String,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| Self {
            main_app,
            state,
            version,
        })
    }

    fn set_state(&mut self, state: CefComponentWindowState, cx: &mut gpui::Context<Self>) {
        self.state = state;
        cx.notify();
    }

    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.start_cef_component_install(cx);
        });
    }
}

impl Render for GpuiCefComponentWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (title, detail, progress, failed) = match &self.state {
            CefComponentWindowState::Installing(component_progress) => {
                let (detail, progress) = match component_progress.phase {
                    component_store::ComponentStoreProgressPhase::Checking => {
                        ("Checking the required runtime…".to_string(), 0.10)
                    }
                    component_store::ComponentStoreProgressPhase::Downloading => {
                        let download_ratio = if component_progress.size_bytes == 0 {
                            0.0
                        } else {
                            component_progress.downloaded_bytes as f32
                                / component_progress.size_bytes as f32
                        }
                        .clamp(0.0, 1.0);
                        let detail = if component_progress.size_bytes == 0 {
                            "Downloading the browser runtime…".to_string()
                        } else {
                            format!(
                                "Downloading the browser runtime… {}% · {} of {}",
                                (download_ratio * 100.0).round() as u32,
                                cef_component_bytes_label(component_progress.downloaded_bytes),
                                cef_component_bytes_label(component_progress.size_bytes),
                            )
                        };
                        (detail, 0.10 + download_ratio * 0.50)
                    }
                    component_store::ComponentStoreProgressPhase::Verifying => {
                        ("Verifying the signed download…".to_string(), 0.65)
                    }
                    component_store::ComponentStoreProgressPhase::Installing => {
                        ("Installing the verified runtime…".to_string(), 0.82)
                    }
                    component_store::ComponentStoreProgressPhase::Pruning => {
                        ("Finishing installation…".to_string(), 0.94)
                    }
                    component_store::ComponentStoreProgressPhase::Ready => {
                        ("Starting Ghostex…".to_string(), 1.0)
                    }
                };
                ("Preparing Ghostex", detail, progress, false)
            }
            CefComponentWindowState::Failed(message) => (
                "Chromium runtime download failed",
                message.clone(),
                0.0,
                true,
            ),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x17191d))
            .p(px(28.0))
            .gap(px(14.0))
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff).opacity(0.95))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(rgb(0xffffff).opacity(0.62))
                    .child(if failed {
                        "Ghostex needs Chromium Embedded Framework before browser surfaces can start. Check your connection to GitHub, then retry."
                    } else {
                        "Ghostex uses a shared Chromium runtime. It is downloaded once for this CEF version and reused by future app updates."
                    }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(0xffffff).opacity(0.42))
                    .child(format!("CEF {}", self.version)),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded(px(3.0))
                    .bg(rgb(0xffffff).opacity(0.08))
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress))
                            .rounded(px(3.0))
                            .bg(if failed {
                                rgb(0xe15a64)
                            } else {
                                rgb(0x4f8fe8)
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(if failed {
                        rgb(0xff9da5).into()
                    } else {
                        rgb(0xffffff).opacity(0.58)
                    })
                    .child(detail),
            )
            .when(failed, |this| {
                this.child(
                    div()
                        .id("ghostex-gpui-cef-component-retry")
                        .flex()
                        .h(px(30.0))
                        .w(px(92.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(rgb(0xffffff).opacity(0.22))
                        .bg(rgb(0x377bd8))
                        .hover(|this| this.bg(rgb(0x4389e7)))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xffffff).opacity(0.94))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.retry(cx);
                            }),
                        )
                        .child("Retry"),
                )
            })
    }
}

fn cef_component_bytes_label(bytes: u64) -> String {
    const MEBIBYTE: f64 = 1024.0 * 1024.0;
    let mebibytes = bytes as f64 / MEBIBYTE;
    if mebibytes >= 10.0 {
        format!("{mebibytes:.0} MB")
    } else {
        format!("{mebibytes:.1} MB")
    }
}

impl GhostexGpuiApp {
    /// The Linux bootstrap's start: install the component when it is missing, then launch the
    /// runtime. The app itself starts CEF through `start_web_runtime` (app/web_runtime_install.rs),
    /// which never downloads on its own (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs).
    #[allow(dead_code)] // the ghostex-gpui-cef-bootstrap binary (Linux) is the only caller
    pub(super) fn begin_cef_startup(&mut self, cx: &mut gpui::Context<Self>) {
        match verified_cef_runtime_readiness() {
            Ok(CefRuntimeReadiness::Ready) => self.initialize_cef(cx),
            Ok(CefRuntimeReadiness::InstallRequired { version }) => {
                self.open_cef_component_window(
                    CefComponentWindowState::Installing(CefComponentProgress::checking()),
                    version,
                    cx,
                );
                self.start_cef_component_install(cx);
            }
            Err(message) => {
                self.open_cef_component_window(
                    CefComponentWindowState::Failed(message),
                    expected_cef_component_version(),
                    cx,
                );
            }
        }
    }

    #[allow(dead_code)] // the ghostex-gpui-cef-bootstrap binary (Linux) is the only caller
    fn open_cef_component_window(
        &mut self,
        state: CefComponentWindowState,
        version: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(handle) = self.cef_component_window {
            if handle
                .update(cx, |view, window, cx| {
                    view.version.clone_from(&version);
                    view.set_state(state.clone(), cx);
                    window.activate_window();
                    window.refresh();
                })
                .is_ok()
            {
                return;
            }
            self.cef_component_window = None;
        }

        let window_size = size(
            px(CEF_COMPONENT_WINDOW_WIDTH),
            px(CEF_COMPONENT_WINDOW_HEIGHT),
        );
        let options = WindowOptions {
            /*
            This window only ever opens in the ghostex-gpui-cef-bootstrap
            process, which installs CEF before the real app exists. There is no
            main window to center against there, so it centers on the primary
            display.
            */
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
                None,
                window_size,
                cx,
            ))),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Ghostex Setup".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };
        let main_app = cx.weak_entity();
        self.cef_component_window = cx
            .open_window(options, move |window, cx| {
                window.activate_window();
                GpuiCefComponentWindow::new(main_app, state, version, cx)
            })
            .ok();
    }

    #[allow(dead_code)] // used by the ghostex-gpui-cef-bootstrap binary, which #[path]-includes this module
    pub(super) fn show_cef_startup_failure(
        &mut self,
        message: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_cef_component_window(
            CefComponentWindowState::Failed(message),
            expected_cef_component_version(),
            cx,
        );
    }

    #[allow(dead_code)] // the ghostex-gpui-cef-bootstrap binary (Linux) is the only caller
    fn start_cef_component_install(&mut self, cx: &mut gpui::Context<Self>) {
        self.cef_component_install_generation =
            self.cef_component_install_generation.wrapping_add(1);
        let generation = self.cef_component_install_generation;
        let version = expected_cef_component_version();
        self.open_cef_component_window(
            CefComponentWindowState::Installing(CefComponentProgress::checking()),
            version,
            cx,
        );

        let (progress_tx, mut progress_rx) = mpsc::unbounded();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = progress_rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if this.cef_component_install_generation != generation {
                        return;
                    }
                    if let Some(handle) = this.cef_component_window {
                        let _ = handle.update(cx, |view, window, cx| {
                            view.set_state(CefComponentWindowState::Installing(progress), cx);
                            window.refresh();
                        });
                    }
                });
            }
        })
        .detach();

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { install_and_verify_cef_component(progress_tx) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.cef_component_install_generation != generation {
                    return;
                }
                match result {
                    Ok(()) => {
                        configure_cef_framework_path_for_process();
                        if let Some(handle) = this.cef_component_window.take() {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        }
                        this.initialize_cef(cx);
                    }
                    Err(message) => {
                        if let Some(handle) = this.cef_component_window {
                            let _ = handle.update(cx, |view, window, cx| {
                                view.set_state(
                                    CefComponentWindowState::Failed(message.clone()),
                                    cx,
                                );
                                window.refresh();
                            });
                        } else {
                            this.open_cef_component_window(
                                CefComponentWindowState::Failed(message),
                                expected_cef_component_version(),
                                cx,
                            );
                        }
                    }
                }
            });
        })
        .detach();
    }
}

pub(crate) enum CefRuntimeReadiness {
    Ready,
    InstallRequired {
        #[allow(dead_code)] // read by the Linux bootstrap's "Preparing Ghostex" window
        version: String,
    },
}

/// Points this process at the CEF runtime to use (the one next to the app, `GHOSTEX_CEF_DIR`, or
/// the component store's current version) and reports whether one was found. A cheap path check:
/// the full verification runs in `verified_cef_runtime_readiness` when CEF starts. On Linux a
/// folder outside the component store also has to hold this build's CEF.
pub(crate) fn configure_cef_framework_path_for_process() -> bool {
    #[cfg(target_os = "linux")]
    let runtime_dir = match linux_cef_runtime_source_for_process().0 {
        LinuxCefRuntimeSource::Folder(runtime_dir) => Some(runtime_dir),
        LinuxCefRuntimeSource::Component => installed_cef_component_dir(),
    };
    #[cfg(not(target_os = "linux"))]
    let runtime_dir = bundled_cef_runtime_dir()
        .or_else(|| {
            let configured = env::var_os(CEF_RUNTIME_DIR_ENV).map(PathBuf::from)?;
            verify_cef_runtime_dir(&configured).ok()?;
            Some(configured)
        })
        .or_else(installed_cef_component_dir);
    let found = runtime_dir.is_some();
    unsafe {
        if let Some(runtime_dir) = runtime_dir {
            env::set_var(CEF_RUNTIME_DIR_ENV, &runtime_dir);
            #[cfg(target_os = "macos")]
            env::set_var(
                CEF_FRAMEWORK_EXECUTABLE_ENV,
                runtime_dir.join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH),
            );
        } else {
            env::remove_var(CEF_RUNTIME_DIR_ENV);
            #[cfg(target_os = "macos")]
            env::remove_var(CEF_FRAMEWORK_EXECUTABLE_ENV);
        }
    }
    found
}

/// The component store's current CEF, when it is installed and is the version this build needs.
fn installed_cef_component_dir() -> Option<PathBuf> {
    let store = on_demand_component_store().ok().flatten()?;
    let component = store.component(CEF_COMPONENT)?;
    if component.component_version != expected_cef_component_version() {
        return None;
    }
    let installed = store.query_current(CEF_COMPONENT).ok()?;
    installed.installed.then_some(installed.path)
}

pub(crate) fn verified_cef_runtime_readiness() -> Result<CefRuntimeReadiness, String> {
    #[cfg(target_os = "linux")]
    {
        let (source, skipped) = linux_cef_runtime_source_for_process();
        return linux_cef_runtime_readiness(source, &skipped, component_cef_runtime_readiness);
    }
    #[cfg(not(target_os = "linux"))]
    {
        if let Some(runtime_dir) = bundled_cef_runtime_dir() {
            verify_cef_runtime_dir(&runtime_dir)?;
            return Ok(CefRuntimeReadiness::Ready);
        }
        if let Some(runtime_dir) = env::var_os(CEF_RUNTIME_DIR_ENV).map(PathBuf::from) {
            verify_cef_runtime_dir(&runtime_dir)?;
            return Ok(CefRuntimeReadiness::Ready);
        }
        component_cef_runtime_readiness()
    }
}

/// Whether the sealed CEF component is installed and verified, or still has to be downloaded.
fn component_cef_runtime_readiness() -> Result<CefRuntimeReadiness, String> {
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed CEF component manifest is unavailable.".to_string())?;
    let component = store.component(CEF_COMPONENT).ok_or_else(|| {
        "The sealed manifest does not define the required CEF component.".to_string()
    })?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }
    let installed = store.query_current(CEF_COMPONENT)?;
    if !installed.installed {
        return Ok(CefRuntimeReadiness::InstallRequired {
            version: component.component_version.clone(),
        });
    }
    verify_cef_runtime_dir(&installed.path)?;
    Ok(CefRuntimeReadiness::Ready)
}

fn install_and_verify_cef_component(
    progress_tx: mpsc::UnboundedSender<CefComponentProgress>,
) -> Result<(), String> {
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed CEF component manifest is unavailable.".to_string())?;
    let component = store.component(CEF_COMPONENT).ok_or_else(|| {
        "The sealed manifest does not define the required CEF component.".to_string()
    })?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }

    let current = store.query_current(CEF_COMPONENT)?;
    if current.installed && verify_cef_runtime_dir(&current.path).is_err() {
        store.uninstall(CEF_COMPONENT, &current.version)?;
    }

    let mut report_progress = |progress: component_store::ComponentStoreProgress| {
        let _ = progress_tx.unbounded_send(CefComponentProgress {
            downloaded_bytes: progress.downloaded_bytes,
            phase: progress.phase,
            size_bytes: progress.size_bytes,
        });
    };
    let installed = store.install(CEF_COMPONENT, &mut report_progress)?;
    verify_cef_runtime_dir(&installed.path)
}

/// Installs the CEF component for the app's Install / Reinstall (web views and Plugins). A plain
/// install keeps a current copy that still verifies and replaces a broken one; `reinstall` always
/// downloads a fresh copy.
#[allow(dead_code)] // not used by the ghostex-gpui-cef-bootstrap binary
pub(crate) fn install_and_verify_cef_component_phases(
    reinstall: bool,
    progress_tx: mpsc::UnboundedSender<component_store::ComponentStoreProgressPhase>,
) -> Result<(), String> {
    let store = on_demand_component_store()?.ok_or_else(|| {
        "This build has no sealed component manifest, so the web runtime cannot be downloaded."
            .to_string()
    })?;
    let component = store
        .component(CEF_COMPONENT)
        .ok_or_else(|| "The sealed manifest does not define the CEF component.".to_string())?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }
    let current = store.query_current(CEF_COMPONENT)?;
    if current.installed && (reinstall || verify_cef_runtime_dir(&current.path).is_err()) {
        store.uninstall(CEF_COMPONENT, &current.version)?;
    }
    let mut report_progress = |progress: component_store::ComponentStoreProgress| {
        let _ = progress_tx.unbounded_send(progress.phase);
    };
    let installed = store.install(CEF_COMPONENT, &mut report_progress)?;
    verify_cef_runtime_dir(&installed.path)
}

/// Removes the component store's copy of the CEF component. A runtime bundled next to the app
/// (development builds, Linux packages) is not a component and stays.
#[allow(dead_code)] // not used by the ghostex-gpui-cef-bootstrap binary
pub(crate) fn uninstall_cef_component() -> Result<bool, String> {
    let store = on_demand_component_store()?.ok_or_else(|| {
        "This build has no sealed component manifest, so the web runtime is not a removable component."
            .to_string()
    })?;
    let current = store.query_current(CEF_COMPONENT)?;
    if !current.installed {
        return Ok(false);
    }
    store.uninstall(CEF_COMPONENT, &current.version)
}

/// Whether this app carries its own CEF runtime next to the executable (development builds and
/// bundles made without on-demand components), which Uninstall cannot remove.
#[allow(dead_code)] // not used by the ghostex-gpui-cef-bootstrap binary
pub(crate) fn cef_runtime_is_bundled() -> bool {
    #[cfg(target_os = "linux")]
    return bundled_cef_runtime_dir()
        .is_some_and(|dir| cef_runtime_folder_matches_build(&dir, expected_cef_version()).is_ok());
    #[cfg(not(target_os = "linux"))]
    bundled_cef_runtime_dir().is_some()
}

/// Where a Linux process takes its CEF runtime from.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, PartialEq)]
enum LinuxCefRuntimeSource {
    /// A runtime folder beside the app or named by `GHOSTEX_CEF_DIR` that holds this build's CEF.
    Folder(PathBuf),
    /// No such folder: the sealed component is used, or downloaded first.
    Component,
}

/// The first of `candidates` that is a complete runtime folder of the CEF this build was compiled
/// against, and why each folder before it was skipped.
///
/// CDXC:CefRuntime 2026-10-27 WHY:
/// Linux links libcef.so at load time, and the bootstrap puts the chosen folder on `LD_LIBRARY_PATH`, so whatever folder wins here is the libcef the runtime runs. Any libcef.so beside the executable used to win with no version check, ahead of the component store; a CEF 154 payload left in /opt/ghostex by an older flat install then aborted every launch of 10.36.0 (CEF 156) with SIGTRAP (GitHub #339). A folder outside the component store is now used only when its libcef.so is this build's exact CEF (read without loading it, `cef_library_version.rs`); any other is skipped, so release builds fall through to the sealed component, and a build without one reports the mismatch. Component folders are named by their version and checked by digest, so they are not read again.
#[cfg(any(target_os = "linux", test))]
fn linux_cef_runtime_source(
    candidates: impl IntoIterator<Item = PathBuf>,
    expected_version: &str,
) -> (LinuxCefRuntimeSource, Vec<String>) {
    let mut skipped = Vec::new();
    for runtime_dir in candidates {
        if skipped.iter().any(|(dir, _)| dir == &runtime_dir) {
            continue;
        }
        match verify_cef_runtime_files(&runtime_dir)
            .and_then(|()| cef_runtime_folder_matches_build(&runtime_dir, expected_version))
        {
            Ok(()) => {
                return (
                    LinuxCefRuntimeSource::Folder(runtime_dir),
                    skipped.into_iter().map(|(_, reason)| reason).collect(),
                );
            }
            Err(reason) => skipped.push((runtime_dir, reason)),
        }
    }
    (
        LinuxCefRuntimeSource::Component,
        skipped.into_iter().map(|(_, reason)| reason).collect(),
    )
}

/// `linux_cef_runtime_source` for this process: the folder beside the executable, then
/// `GHOSTEX_CEF_DIR`. Each skipped folder is reported on stderr once per process.
#[cfg(target_os = "linux")]
fn linux_cef_runtime_source_for_process() -> (LinuxCefRuntimeSource, Vec<String>) {
    static REPORTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let candidates = bundled_cef_runtime_dir()
        .into_iter()
        .chain(env::var_os(CEF_RUNTIME_DIR_ENV).map(PathBuf::from));
    let (source, skipped) = linux_cef_runtime_source(candidates, expected_cef_version());
    if let Ok(mut reported) = REPORTED.lock() {
        for reason in &skipped {
            if !reported.contains(reason) {
                eprintln!("Ghostex: skipping a Chromium runtime: {reason}");
                reported.push(reason.clone());
            }
        }
    }
    (source, skipped)
}

/// Readiness once the source is known: a matching folder is ready; otherwise the component decides,
/// and when it cannot, the error also names every folder that was skipped.
#[cfg(any(target_os = "linux", test))]
fn linux_cef_runtime_readiness(
    source: LinuxCefRuntimeSource,
    skipped: &[String],
    component_readiness: impl FnOnce() -> Result<CefRuntimeReadiness, String>,
) -> Result<CefRuntimeReadiness, String> {
    match source {
        LinuxCefRuntimeSource::Folder(_) => Ok(CefRuntimeReadiness::Ready),
        LinuxCefRuntimeSource::Component => component_readiness().map_err(|reason| {
            skipped
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(reason.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        }),
    }
}

/// Whether the libcef.so in `runtime_dir` is the CEF build this app was compiled against.
#[cfg(any(target_os = "linux", test))]
fn cef_runtime_folder_matches_build(
    runtime_dir: &Path,
    expected_version: &str,
) -> Result<(), String> {
    let library = runtime_dir.join("libcef.so");
    match crate::cef_library_version::libcef_build_version(&library)? {
        Some(found) if found == expected_version => Ok(()),
        Some(found) => Err(format!(
            "{} is CEF {found}, but this build of Ghostex needs CEF {expected_version}.",
            library.display()
        )),
        None => Err(format!(
            "{} does not name its CEF version, so it cannot be checked against CEF {expected_version}.",
            library.display()
        )),
    }
}

/// The CEF build this app was compiled against, as `cef_version_full()` reports it.
#[cfg(any(target_os = "linux", test))]
fn expected_cef_version() -> &'static str {
    env!("GHOSTEX_CEF_VERSION")
}

fn bundled_cef_runtime_dir() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    #[cfg(target_os = "macos")]
    let path = executable_dir
        .join("../Frameworks")
        .join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH);
    #[cfg(target_os = "macos")]
    return path.is_file().then(|| {
        let runtime_dir = path
            .ancestors()
            .nth(2)
            .expect("CEF framework executable must have a component root");
        runtime_dir
            .canonicalize()
            .unwrap_or_else(|_| runtime_dir.to_path_buf())
    });
    #[cfg(target_os = "windows")]
    return executable_dir
        .join("libcef.dll")
        .is_file()
        .then(|| executable_dir.to_path_buf());
    #[cfg(target_os = "linux")]
    return executable_dir
        .join("libcef.so")
        .is_file()
        .then(|| executable_dir.to_path_buf());
}

/// The files a Linux CEF runtime folder needs, checked by path only.
#[cfg(any(target_os = "linux", test))]
fn verify_cef_runtime_files(runtime_dir: &Path) -> Result<(), String> {
    let library_path = runtime_dir.join("libcef.so");
    if !library_path.is_file() {
        return Err(format!(
            "The verified CEF component is missing its runtime library at {}.",
            library_path.display()
        ));
    }
    for required in [runtime_dir.join("icudtl.dat"), runtime_dir.join("locales")] {
        if !required.exists() {
            return Err(format!(
                "The verified CEF component is missing a required runtime resource at {}.",
                required.display()
            ));
        }
    }
    Ok(())
}

fn verify_cef_runtime_dir(runtime_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    verify_cef_runtime_files(runtime_dir)?;
    #[cfg(target_os = "macos")]
    let library_path = runtime_dir.join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH);
    #[cfg(target_os = "windows")]
    let library_path = runtime_dir.join("libcef.dll");
    #[cfg(not(target_os = "linux"))]
    if !library_path.is_file() {
        return Err(format!(
            "The verified CEF component is missing its runtime library at {}.",
            library_path.display()
        ));
    }
    #[cfg(target_os = "windows")]
    for required in [runtime_dir.join("icudtl.dat"), runtime_dir.join("locales")] {
        if !required.exists() {
            return Err(format!(
                "The verified CEF component is missing a required runtime resource at {}.",
                required.display()
            ));
        }
    }
    #[cfg(target_os = "macos")]
    let framework = library_path
        .parent()
        .ok_or_else(|| "The CEF framework executable has no parent directory.".to_string())?;
    #[cfg(target_os = "macos")]
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(framework)
        .output()
        .map_err(|error| format!("Could not verify the downloaded CEF signature: {error}"))?;
    #[cfg(target_os = "macos")]
    if !verification.status.success() {
        return Err("The downloaded CEF component failed macOS code-signature verification. Retry to download a clean copy.".to_string());
    }

    #[cfg(target_os = "macos")]
    let app_team = code_signing_team_identifier(&env::current_exe().map_err(|error| {
        format!("Could not resolve the Ghostex executable for signature verification: {error}")
    })?)?;
    #[cfg(target_os = "macos")]
    let cef_team = code_signing_team_identifier(framework)?;
    #[cfg(target_os = "macos")]
    if app_team != cef_team {
        return Err(format!(
            "The downloaded CEF component is signed by a different Developer ID team (app: {}, CEF: {}).",
            app_team.as_deref().unwrap_or("ad hoc"),
            cef_team.as_deref().unwrap_or("ad hoc")
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn code_signing_team_identifier(path: &Path) -> Result<Option<String>, String> {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(path)
        .output()
        .map_err(|error| {
            format!(
                "Could not inspect code signature for {}: {error}",
                path.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect code signature for {}.",
            path.display()
        ));
    }
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    Ok(diagnostics.lines().find_map(|line| {
        line.strip_prefix("TeamIdentifier=")
            .filter(|value| *value != "not set")
            .map(str::to_string)
    }))
}

fn expected_cef_component_version() -> String {
    env!("GHOSTEX_CEF_COMPONENT_VERSION")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD_CEF: &str = "156.0.14+g4fa4ab5+chromium-156.0.8078.48";
    const OLD_CEF: &str = "154.0.26+ge72305f+chromium-154.0.8037.58";

    /// A folder of test runtime folders, removed when the test ends.
    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new(name: &str) -> Self {
            let root = env::temp_dir().join(format!(
                "ghostex-cef-runtime-choice-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create the test root");
            Self(root)
        }

        /// A complete Linux CEF runtime folder whose libcef.so is `version`.
        fn runtime(&self, name: &str, version: &str) -> PathBuf {
            let runtime_dir = self.0.join(name);
            std::fs::create_dir_all(runtime_dir.join("locales")).expect("create locales");
            std::fs::write(runtime_dir.join("icudtl.dat"), b"icu").expect("write icudtl.dat");
            std::fs::write(
                runtime_dir.join("libcef.so"),
                crate::cef_library_version::fake_elf_library(format!("\0{version}\0").as_bytes()),
            )
            .expect("write libcef.so");
            runtime_dir
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_stray_libcef_of_another_cef_beside_the_app_is_skipped_for_the_component() {
        let root = TestRoot::new("stray");
        let stray = root.runtime("opt-ghostex", OLD_CEF);
        let (source, skipped) = linux_cef_runtime_source([stray.clone()], BUILD_CEF);
        assert_eq!(source, LinuxCefRuntimeSource::Component);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains(OLD_CEF) && skipped[0].contains(BUILD_CEF));
        assert!(skipped[0].contains(&stray.join("libcef.so").display().to_string()));
        let readiness = linux_cef_runtime_readiness(source, &skipped, || {
            Ok(CefRuntimeReadiness::InstallRequired {
                version: "156.0.14-g4fa4ab5-chromium-156.0.8078.48-stripped".to_string(),
            })
        });
        assert!(matches!(
            readiness,
            Ok(CefRuntimeReadiness::InstallRequired { .. })
        ));
    }

    #[test]
    fn a_bundled_runtime_of_this_builds_cef_is_used() {
        let root = TestRoot::new("bundled");
        let bundled = root.runtime("app", BUILD_CEF);
        let configured = root.runtime("configured", BUILD_CEF);
        let (source, skipped) = linux_cef_runtime_source([bundled.clone(), configured], BUILD_CEF);
        assert_eq!(source, LinuxCefRuntimeSource::Folder(bundled));
        assert!(skipped.is_empty());
    }

    #[test]
    fn the_component_the_bootstrap_chose_wins_over_a_stray_beside_the_app() {
        let root = TestRoot::new("component");
        let stray = root.runtime("opt-ghostex", OLD_CEF);
        let component = root.runtime("components/cef/156", BUILD_CEF);
        let (source, skipped) = linux_cef_runtime_source([stray, component.clone()], BUILD_CEF);
        assert_eq!(source, LinuxCefRuntimeSource::Folder(component));
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn an_incomplete_or_unversioned_runtime_folder_is_skipped() {
        let root = TestRoot::new("incomplete");
        let incomplete = root.runtime("incomplete", BUILD_CEF);
        std::fs::remove_file(incomplete.join("icudtl.dat")).unwrap();
        let unversioned = root.runtime("unversioned", "");
        let (source, skipped) = linux_cef_runtime_source([incomplete, unversioned], BUILD_CEF);
        assert_eq!(source, LinuxCefRuntimeSource::Component);
        assert_eq!(skipped.len(), 2);
        assert!(skipped[0].contains("icudtl.dat"));
        assert!(skipped[1].contains("does not name its CEF version"));
    }

    #[test]
    fn without_a_usable_cef_the_error_names_the_skipped_runtime_and_the_component() {
        let root = TestRoot::new("none");
        let stray = root.runtime("opt-ghostex", OLD_CEF);
        let (source, skipped) = linux_cef_runtime_source([stray], BUILD_CEF);
        let error = linux_cef_runtime_readiness(source, &skipped, || {
            Err("The sealed CEF component manifest is unavailable.".to_string())
        })
        .err()
        .expect("no usable CEF must be an error");
        assert!(error.contains(OLD_CEF), "{error}");
        assert!(error.ends_with("The sealed CEF component manifest is unavailable."));

        let (source, skipped) = linux_cef_runtime_source(Vec::<PathBuf>::new(), BUILD_CEF);
        let error = linux_cef_runtime_readiness(source, &skipped, || {
            Err("The sealed CEF component manifest is unavailable.".to_string())
        })
        .err()
        .expect("no CEF at all must be an error");
        assert_eq!(error, "The sealed CEF component manifest is unavailable.");
    }

    #[test]
    fn this_build_expects_the_cef_its_bindings_name() {
        let expected = expected_cef_version();
        assert!(
            expected_cef_component_version().starts_with(&expected.replace('+', "-")),
            "{expected} vs {}",
            expected_cef_component_version()
        );
    }
}

use gpui::App;

/// CDXC:DesignSystem 2026-09-26 DECISION:
/// User: Windows should render with the same quality, type sizes and spacing as macOS.
pub(crate) const UI_FONT: &str = "Inter";

/// The Files view's type sizes: its editor text, its line numbers and its file list names. The
/// Changes view draws its diff with the same three, so both views read at one size.
pub(crate) const FILES_TEXT_SIZE: f32 = 14.0;
pub(crate) const FILES_LINE_NUMBER_SIZE: f32 = 12.0;
/// `.manage-file-name`: 15.55px at weight 300.
pub(crate) const FILES_NAME_SIZE: f32 = 15.55;

/// CDXC:DesignSystem 2026-09-26 WHY:
/// The bundled static faces identify as Inter, not Inter Variable. Register them before the first window so chrome and Docs use the same metrics without relying on fonts installed on the host.
pub(crate) fn register(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let fonts = vec![
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Light.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Medium.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-SemiBold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Bold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Italic.ttf")
                .as_slice()
                .into(),
        ];
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("[ghostex-gpui] Could not register UI fonts: {error}");
        }
    });
}

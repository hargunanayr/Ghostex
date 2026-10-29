//! A session's PR, issue and Linear chips: the line and the badge a sidebar row shows (as the
//! "Session links in the sidebar" setting says), and the chip drawing the work area header shares
//! (render/workarea_header/session_links.rs). Hovering a row opens the floating links card
//! (window/session_links_card.rs on the desktop).
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "also didn't we agree to have a way to see these floating on the right of the sidebar when i hover somewhere? (instead of showing them just below the sidebar item". Hovering a row that has links opens a card beside the sidebar listing them all, after the tooltip delay; the row no longer grows by a line on hover. This supersedes the earlier 2026-10-24 rule that a hovered card expands to show its links below it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/links.rs (`SessionLinksDisplay` and what a row
//! shows, glyphs, tones and words), apps/desktop/src/app/gx_store/session_links.rs (what a click
//! sends), apps/mobile/app/src/chat/native/sessionLinks.ts (the phone's copy of the row rule).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, TextRun, WindowTextSystem, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::tooltips::SidebarTooltipSpan;
pub(crate) use super::tooltips::{sidebar_style_card_tooltip, sidebar_style_tooltip};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::{
    SessionLink, SessionLinkGlyph, SessionLinkTone, SessionLinks, SessionLinksDisplay,
    SessionLinksRow,
};

/// The line's height, the Work mode card's chip line's (work_chips.rs).
pub(crate) const LINK_LINE_HEIGHT: f32 = 16.0;
/// A text's width in the sidebar's UI font at `size`, unscaled.
fn ui_text_width(text_system: &WindowTextSystem, text: &str, size: f32) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(crate::ui_fonts::UI_FONT),
        ..Default::default()
    };
    f32::from(
        text_system
            .shape_line(text.to_owned().into(), px(size), &[run], None)
            .width,
    )
}

/// How wide `session_link_chip_body` draws, unscaled: glyph, gap, label, and a PR's checks mark.
fn link_chip_width(text_system: &WindowTextSystem, link: &SessionLink, text_size: f32) -> f32 {
    12.0 + 3.0
        + ui_text_width(text_system, &link.label, text_size)
        + if link.checks.is_some() {
            3.0 + 12.0
        } else {
            0.0
        }
}

/// How wide `session_links_overflow_pill` draws for `hidden`, unscaled: its padding and border,
/// "+N", and the failing dot.
fn overflow_pill_width(
    text_system: &WindowTextSystem,
    hidden: usize,
    failing: bool,
    text_size: f32,
) -> f32 {
    2.0 * 6.0
        + 2.0
        + ui_text_width(text_system, &format!("+{hidden}"), text_size)
        + if failing { 4.0 + 5.0 } else { 0.0 }
}

pub(crate) fn session_link_glyph_icon(glyph: SessionLinkGlyph) -> &'static str {
    match glyph {
        SessionLinkGlyph::Branch => "titlebar/git-branch.svg",
        SessionLinkGlyph::Github => "titlebar/brand-github.svg",
        SessionLinkGlyph::Linear => "titlebar/brand-linear.svg",
        SessionLinkGlyph::ChecksPassing => "titlebar/circle-check.svg",
        SessionLinkGlyph::ChecksFailing => "titlebar/circle-x.svg",
        SessionLinkGlyph::ChecksPending => "titlebar/loader2.svg",
    }
}

/// The work chips' colours (work_chips.rs), the same in both themes.
pub(crate) fn session_link_tone_color(tone: SessionLinkTone) -> Hsla {
    rgb(match tone {
        SessionLinkTone::Open => 0x3fb950,
        SessionLinkTone::Draft => 0x8b949e,
        SessionLinkTone::Merged => 0xa371f7,
        SessionLinkTone::Closed => 0xf85149,
        SessionLinkTone::Pending => 0xd29922,
        SessionLinkTone::Started => 0xf2c94c,
        SessionLinkTone::Review => 0x4cb782,
        SessionLinkTone::Done => 0x5e6ad2,
        SessionLinkTone::Linear => 0x7c84f0,
    })
    .into()
}

/// What clicking a chip sends: the link opens where Settings says (gx_store/session_links.rs).
pub(crate) fn open_session_link_command(link: &SessionLink) -> Option<serde_json::Value> {
    link.url
        .as_ref()
        .map(|url| json!({ "type": "openSessionLink", "url": url }))
}

/// One chip: the state glyph, the number or ID, and a PR's checks mark.
pub(crate) fn session_link_chip_body(
    link: &SessionLink,
    scale: f32,
    text_size: f32,
    muted: Hsla,
) -> gpui::Div {
    let (glyph, tone) = link.glyph();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .text_size(px(text_size * scale))
        .text_color(muted)
        .whitespace_nowrap()
        .child(titlebar_svg_icon(
            session_link_glyph_icon(glyph),
            12.0 * scale,
            session_link_tone_color(tone),
        ))
        .child(link.label.clone())
        .when_some(link.checks_mark(), |chip, (glyph, tone)| {
            chip.child(titlebar_svg_icon(
                session_link_glyph_icon(glyph),
                12.0 * scale,
                session_link_tone_color(tone),
            ))
        })
}

/// The Badge option's count: a link icon and the number, in `color`; dashed for suggestions.
fn session_links_badge_body(count: usize, color: Hsla, dashed: bool, scale: f32) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .h(px(16.0 * scale))
        .px(px(5.0 * scale))
        .rounded_full()
        .border_1()
        .when(dashed, |badge| badge.border_dashed())
        .border_color(color.opacity(0.45))
        .text_size(px(10.5 * scale))
        .text_color(color)
        .child(titlebar_svg_icon("titlebar/link.svg", 10.0 * scale, color))
        .child(count.to_string())
}

/// The "+N" pill: the links that did not fit, with a red dot when one of them has failing checks.
///
/// CDXC:SessionLinks 2026-10-24 SEE-ALSO: the user's "dot-only" answer (server/src/server/session_links_sync.rs); this dot is how a failing PR behind "+N" still shows.
pub(crate) fn session_links_overflow_pill(
    hidden: usize,
    failing: bool,
    scale: f32,
    text_size: f32,
    foreground: Hsla,
    border: Hsla,
) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0 * scale))
        .h(px(15.0 * scale))
        .px(px(6.0 * scale))
        .rounded_full()
        .border_1()
        .border_color(border)
        .text_size(px(text_size * scale))
        .text_color(foreground)
        .whitespace_nowrap()
        .child(format!("+{hidden}"))
        .when(failing, |pill| {
            pill.child(
                div()
                    .size(px(5.0 * scale))
                    .rounded_full()
                    .bg(session_link_tone_color(SessionLinkTone::Closed)),
            )
        })
}

impl GhostexGpuiApp {
    /// A session's links as gxserver last published them.
    pub(crate) fn session_links_of(
        &self,
        key: &ghostex_gx_core::SessionKey,
    ) -> Option<SessionLinks> {
        let session = self.gx_store.core.presentation().session(key)?;
        session
            .links
            .as_ref()
            .map(SessionLinks::from_presentation)
            .filter(|links| !links.is_empty())
    }

    /// The "Session links in the sidebar" setting.
    pub(crate) fn native_session_links_display(&self) -> SessionLinksDisplay {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        SessionLinksDisplay::from_setting(
            settings
                .object()
                .get(SessionLinksDisplay::SETTING_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }

    /// What this row shows of its links (gx-core's rule), the session open in the window counting
    /// as selected.
    pub(crate) fn native_session_links_row(
        &self,
        session: &NativeSidebarSession,
    ) -> SessionLinksRow {
        let Some(links) = session.links.as_deref() else {
            return SessionLinksRow::default();
        };
        let (selected, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            session.is_browser(),
            session.is_focused,
            session.is_visible,
        );
        links.row(self.native_session_links_display(), selected)
    }

    /// Whether the row draws a second line: its links line, or a Work mode card's Clean up / Keep
    /// offer for a merged PR (work_chips.rs), which takes the line while it waits. In the
    /// two-column layout every card has one: when neither of those takes it, the status, agent and
    /// branch line (two_column/card_meta.rs).
    pub(crate) fn native_session_has_second_line(&self, session: &NativeSidebarSession) -> bool {
        self.native_sidebar.two_column.active || self.native_session_has_link_line(session)
    }

    /// Whether the links line or the Work mode offer takes the row's second line.
    pub(crate) fn native_session_has_link_line(&self, session: &NativeSidebarSession) -> bool {
        let row = self.native_session_links_row(session);
        session.work.as_ref().is_some_and(|work| work.offer_cleanup)
            || !row.line.is_empty()
            || row.badge.is_some_and(|badge| badge.suggested)
    }

    /// The badge a row's title shows with the Badge option: the number of links (the ones not
    /// pinned), coloured by the most pressing one. Hidden while the row is hovered, like the row's
    /// other badges. A session's quiet suggestions badge sits on the line under its name instead.
    pub(super) fn render_native_session_links_badge(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        let badge = self.native_session_links_row(session).badge?;
        // The quiet suggestions badge sits on the line under the name instead
        // (`render_native_session_link_line`).
        if badge.suggested {
            return None;
        }
        let color = if badge.suggested {
            appearance.muted
        } else {
            session_link_tone_color(badge.tone)
        };
        Some(
            session_links_badge_body(badge.count, color, badge.suggested, appearance.scale)
                .into_any_element(),
        )
    }

    /// The row's links line (the Always, Selected session and Open work only options, and the
    /// pinned links under every option but Off): the chips gx-core picked that fit, pinned first,
    /// then "+N" for the rest (the badge under Badge), which opens the floating card.
    pub(super) fn render_native_session_link_line(
        &self,
        session: &NativeSidebarSession,
        indent: f32,
        tooltip_span: SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if session.work.as_ref().is_some_and(|work| work.offer_cleanup) {
            return None;
        }
        let row = self.native_session_links_row(session);
        let scale = appearance.scale;
        if row.line.is_empty() {
            // CDXC:SessionLinks 2026-10-24 DECISION:
            // User: "show the option to link a thread with issues/prs etc under the name of the thread instead of on the same thread title row", which they narrowed to "i said that for the case where it's showing me the link icon and telling me there's suggested links". The quiet suggestions badge of a session with suggestions but no links sits on a line under the session's name, where its chips would be; the regular Badge for linked links stays on the title row.
            let badge = row.badge.filter(|badge| badge.suggested)?;
            return Some(
                h_flex()
                    .h(px(LINK_LINE_HEIGHT * scale))
                    .flex_shrink_0()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    // Under the title.
                    .pl(px((indent + 15.0 + 6.0) * scale))
                    .child(
                        session_links_badge_body(badge.count, appearance.muted, true, scale)
                            .id(format!(
                                "native-session-links-suggested-{}",
                                session.session_id
                            ))
                            .role(gpui::Role::Image)
                            .aria_label(format!("{} suggested links", badge.count)),
                    )
                    .into_any_element(),
            );
        }
        let links: &SessionLinks = session.links.as_deref()?;
        let items: Vec<SessionLink> = row
            .line
            .iter()
            .filter_map(|index| links.items.get(*index).cloned())
            .collect();
        // The card's width as it last painted, kept in step with the sidebar's width (the card's
        // inset from the sidebar's edges does not change while the sidebar is dragged wider),
        // less the line's indent under the title and its right padding.
        let card_width = match self
            .native_sidebar
            .session_card_bounds
            .get(&session.session_id)
        {
            Some(card) => {
                let inset =
                    f32::from(self.native_sidebar.bounds.size.width) - f32::from(card.size.width);
                self.sidebar_width - inset
            }
            None => {
                // The row has not painted yet: draw once with the sidebar's own width less the
                // usual insets, then again with the card's.
                let app = cx.entity();
                cx.defer(move |cx| app.update(cx, |_, cx| cx.notify()));
                self.sidebar_width - 40.0 * scale
            }
        };
        let available = card_width / scale - (indent + 15.0 + 6.0) - 6.0;
        let text_system = WindowTextSystem::new(cx.text_system().clone());
        let chip_widths: Vec<f32> = items
            .iter()
            .map(|link| link_chip_width(&text_system, link, 11.5))
            .collect();
        // Under Badge the title row's badge counts the links the pinned chips leave out.
        let never_shown = if row.badge.is_some() {
            0
        } else {
            links.items.len() - items.len()
        };
        let failing_beyond = |shown: usize| {
            links.items.iter().any(|link| {
                link.checks.as_deref() == Some("failing")
                    && !items.iter().take(shown).any(|item| item.key == link.key)
            })
        };
        let shown = ghostex_gx_core::link_chips_that_fit(
            &chip_widths,
            never_shown,
            available,
            10.0,
            |hidden| {
                let shown = (items.len() + never_shown).saturating_sub(hidden);
                overflow_pill_width(&text_system, hidden, failing_beyond(shown), 11.0)
            },
        );
        let hidden = items.len() + never_shown - shown;
        let show_tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let session_id = session.session_id.clone();
        let chips = items.iter().take(shown).enumerate().map(|(index, link)| {
            let click = open_session_link_command(link);
            let tooltip = link.hover_card_text();
            let foreground = appearance.foreground;
            session_link_chip_body(link, scale, 11.5, appearance.muted)
                .id(format!("native-session-link-{index}-{session_id}"))
                .role(gpui::Role::Link)
                .aria_label(link.summary())
                .when_some(click, |chip, click| {
                    chip.cursor_pointer()
                        .hover(move |style| {
                            style
                                .text_color(foreground)
                                .text_decoration_1()
                                .text_decoration_color(foreground)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(click.clone(), cx);
                        }))
                })
                .when(show_tooltips, |chip| {
                    chip.managed_discrete_tooltip_with_placement(
                        tooltip_span.placement(),
                        appearance.tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_tooltip(
                                tooltip.clone(),
                                tooltip_span,
                                scale,
                                window,
                                cx,
                            )
                        },
                    )
                })
                .into_any_element()
        });
        let overflow = (hidden > 0).then(|| {
            let key = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session_id)
                .filter(|key| key.machine.is_local());
            let failing = links.items.iter().any(|link| {
                link.checks.as_deref() == Some("failing")
                    && !items.iter().take(shown).any(|shown| shown.key == link.key)
            });
            session_links_overflow_pill(
                hidden,
                failing,
                scale,
                11.0,
                appearance.foreground,
                appearance.muted.opacity(0.45),
            )
            .id(format!("native-session-links-more-{session_id}"))
            .role(gpui::Role::Button)
            .aria_label(format!("{hidden} more links"))
            .when(key.is_some(), |pill| {
                let session_id = session_id.clone();
                pill.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.open_session_links_card(session_id.clone(), cx);
                    }))
            })
            .into_any_element()
        });
        Some(
            h_flex()
                .h(px(LINK_LINE_HEIGHT * scale))
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                // Under the title.
                .pl(px((indent + 15.0 + 6.0) * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .children(chips)
                .children(overflow)
                .into_any_element(),
        )
    }
}

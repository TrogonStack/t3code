//! The web's CSS color tokens for the Settings route, resolved for the default theme.
//!
//! The route sits outside `[data-app-sidebar]`, so the `:root` blocks of `index.css` apply
//! (light 1113-1183, dark 1189-1228), not the sidebar overrides at 1231-1275. gpui-component's
//! theme fields differ from these, so the route reads this table instead.

use gpui::{App, Hsla, rgb, white};
use gpui_component::ActiveTheme as _;

#[derive(Clone, Copy)]
pub(super) struct Tokens {
    pub dark: bool,
    pub background: Hsla,
    pub foreground: Hsla,
    pub card: Hsla,
    pub popover: Hsla,
    pub primary: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    pub destructive: Hsla,
    pub sidebar: Hsla,
    pub sidebar_row_hover: Hsla,
    pub sidebar_row_selected: Hsla,
    /// `--sidebar-icon-color`: `sidebar-muted-foreground` mixed 85% into `--sidebar` (index.css:106).
    pub sidebar_icon: Hsla,
}

fn mix(a: u32, b: u32, share_of_a: f32) -> Hsla {
    let channel = |shift: u32| {
        let (a, b) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        (a * share_of_a + b * (1. - share_of_a)).round() as u32
    };
    rgb((channel(16) << 16) | (channel(8) << 8) | channel(0)).into()
}

impl Tokens {
    pub(super) fn of(cx: &App) -> Self {
        if cx.theme().is_dark() {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// `:root` dark variant, index.css:1189-1228.
    fn dark() -> Self {
        // neutral-950 mixed 97% with white (`--card`, `--popover`).
        let card = mix(0x0a0a0a, 0xffffff, 0.97);
        Self {
            dark: true,
            background: rgb(0x000000).into(),
            foreground: rgb(0xf5f5f5).into(),
            card,
            popover: card,
            primary: rgb(0x346bf1).into(),
            muted_foreground: rgb(0xa3a3a3).into(),
            accent: white().opacity(0.04),
            accent_foreground: rgb(0xf5f5f5).into(),
            border: white().opacity(0.08),
            input: white().opacity(0.11),
            ring: rgb(0x346bf1).into(),
            destructive: mix(0xef4444, 0xffffff, 0.9),
            sidebar: card,
            sidebar_row_hover: white().opacity(0.04),
            sidebar_row_selected: white().opacity(0.03),
            sidebar_icon: mix(0xa3a3a3, 0x111111, 0.85),
        }
    }

    /// `:root` light, index.css:1113-1183.
    fn light() -> Self {
        Self {
            dark: false,
            background: rgb(0xfcfcfc).into(),
            foreground: rgb(0x18181b).into(),
            card: rgb(0xffffff).into(),
            popover: rgb(0xffffff).into(),
            primary: rgb(0x1b4ed8).into(),
            muted_foreground: rgb(0x71717a).into(),
            accent: rgb(0xf4f4f5).into(),
            accent_foreground: rgb(0x18181b).into(),
            border: rgb(0xe4e4e7).into(),
            input: rgb(0xd4d4d8).into(),
            ring: rgb(0x1b4ed8).into(),
            destructive: rgb(0xef4444).into(),
            sidebar: rgb(0xfafafa).into(),
            sidebar_row_hover: rgb(0xfcfcfc).into(),
            sidebar_row_selected: rgb(0xffffff).into(),
            sidebar_icon: mix(0x71717a, 0xfafafa, 0.85),
        }
    }

    /// `dialog-backdrop` (index.css:444-452): `--background` at 60%, 64% in dark.
    pub(super) fn backdrop(&self) -> Hsla {
        self.background.opacity(if self.dark { 0.64 } else { 0.6 })
    }
}

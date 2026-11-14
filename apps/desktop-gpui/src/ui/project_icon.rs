//! The one project icon, a port of the web's `ProjectFavicon`: the override's monogram, emoji or
//! lucide icon, else the project's favicon image, else the monogram of its title. A lucide name
//! this app does not know draws `folder-code`, and an image that fails to load draws the monogram.

use crate::assets::lucide_path;
use crate::favicon;
use crate::projects::{IconKind, ProjectIcon, palette_hex};
use gpui::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ObjectFit, ParentElement, SharedString, Styled,
    StyledImage, div, img, px, rgb,
};
use gpui_component::{ActiveTheme, Icon};

/// `icon` at `size` px: `size-4` in the sidebar and lists, a little larger where the web is.
pub fn project_icon(icon: &ProjectIcon, size: f32, cx: &App) -> AnyElement {
    let dark = cx.theme().is_dark();
    let mono = cx.theme().mono_font_family.clone();
    let color = |index: usize| -> Hsla { rgb(palette_hex(index, dark)).into() };
    match &icon.kind {
        IconKind::Monogram { text, color: index } => {
            monogram(text, color(*index), size, mono).into_any_element()
        }
        IconKind::Emoji(emoji) => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(size))
            .text_size(px(size * 0.8))
            .line_height(px(size * 0.8))
            .child(emoji.clone())
            .into_any_element(),
        IconKind::Lucide { name, color: index } => Icon::empty()
            .path(lucide_path(name))
            .size(px(size))
            .flex_shrink_0()
            .text_color(color(*index))
            .into_any_element(),
        IconKind::Auto => {
            let (text, index) = &icon.fallback;
            let fallback = {
                let (text, color, mono) = (text.clone(), color(*index), mono.clone());
                move || monogram(&text, color, size, mono.clone()).into_any_element()
            };
            let Some(image) = icon.favicon.as_ref().and_then(|f| favicon::lookup(f, cx)) else {
                return fallback();
            };
            img(image)
                .flex_shrink_0()
                .size(px(size))
                .rounded(px(size / 4.))
                .object_fit(ObjectFit::Contain)
                .with_loading(fallback.clone())
                .with_fallback(fallback)
                .into_any_element()
        }
    }
}

/// `ProjectMonogram`: the text on its color at 14%, 8.25 px of a 16 px tile.
fn monogram(text: &str, color: Hsla, size: f32, font: SharedString) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .text_size(px(size * 8.25 / 16.))
        .font_weight(FontWeight::BOLD)
        .font_family(font)
        .child(text.to_owned())
}

//! The web's `blur-xs` over a redacted placeholder. GPUI cannot blur glyphs, so each character
//! keeps its advance in transparent text and paints a soft blob in its ink area instead; the blobs
//! smear together like the CSS blur and give away nothing but the shape of the value.

use gpui::{
    BoxShadow, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, point, px,
    relative, transparent_black,
};

const GROUP: &str = "blurred-text";

/// `text` (already a placeholder) blurred in `color`, and in `hover` under the pointer like the
/// web's `hover:text-foreground`. Font and size come from the parent.
pub fn blurred_text(text: &str, color: Hsla, hover: Hsla) -> Div {
    div()
        .group(GROUP)
        .flex()
        .min_w_0()
        .children(text.chars().map(|ch| glyph(ch, color, hover)))
}

/// The CSS blur spreads ink well past the glyph and thins it out.
fn smudge(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: color.opacity(0.25),
        offset: point(px(0.), px(0.)),
        blur_radius: px(7.),
        spread_radius: px(1.5),
        inset: false,
    }]
}

fn glyph(ch: char, color: Hsla, hover: Hsla) -> impl IntoElement {
    let (top, bottom, inset) = ink(ch);
    div()
        .relative()
        .flex_shrink_0()
        .child(div().text_color(transparent_black()).child(ch.to_string()))
        .child(
            div()
                .absolute()
                .top(relative(top))
                .bottom(relative(bottom))
                .left(relative(inset))
                .right(relative(inset))
                .rounded_full()
                .bg(color.opacity(0.05))
                .shadow(smudge(color))
                .group_hover(GROUP, move |style| {
                    style.bg(hover.opacity(0.05)).shadow(smudge(hover))
                }),
        )
}

/// Top and bottom of the glyph's ink as fractions of the line box, and its side inset.
fn ink(ch: char) -> (f32, f32, f32) {
    match ch {
        '.' => (0.6, 0.22, 0.3),
        '-' => (0.46, 0.42, 0.1),
        '_' => (0.74, 0.14, 0.0),
        'b' | 'd' | 'f' | 'h' | 'k' | 'l' | 't' | '0'..='9' | 'A'..='Z' => (0.22, 0.22, 0.1),
        'g' | 'j' | 'p' | 'q' | 'y' => (0.38, 0.06, 0.1),
        _ => (0.38, 0.22, 0.1),
    }
}

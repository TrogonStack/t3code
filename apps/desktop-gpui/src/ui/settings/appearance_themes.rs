//! The wireframe and orb drawings of the Appearance tab: `ThemeWireframe.tsx` and
//! `ThemePreviewCircles.tsx`. gpui has no radial gradients, so each orb layer is a stack of
//! concentric translucent circles that steps through the same alpha ramp.

use super::*;
use crate::settings_appearance::{ColorMode, OrbPalette, ThemeChoice, ThemeId, mix};
use gpui::{BoxShadow, Hsla, Rgba, point, relative, rgb};

fn color(rgb: [f32; 3], a: f32) -> Hsla {
    Rgba {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a,
    }
    .into()
}

/// The roles `ThemeWireframePane` paints.
struct Wire {
    canvas: Hsla,
    sidebar: Hsla,
    surface: Hsla,
    accent_surface: Hsla,
    message_surface: Hsla,
    action: Hsla,
}

/// The default theme's roles are exact; the others derive theirs from their orb colors.
fn wire(theme: ThemeId, dark: bool) -> Wire {
    if theme == ThemeId::Default {
        let hex = |value: u32| -> Hsla { rgb(value).into() };
        return if dark {
            Wire {
                canvas: hex(0x0a0a0a),
                sidebar: hex(0x0f0f10),
                surface: hex(0x121212),
                accent_surface: hex(0x27272a),
                message_surface: hex(0x27272a),
                action: hex(0x8b9cff),
            }
        } else {
            Wire {
                canvas: hex(0xfcfcfc),
                sidebar: hex(0xfafafa),
                surface: hex(0xffffff),
                accent_surface: hex(0xf4f4f5),
                message_surface: hex(0xe4e4e7),
                action: hex(0x4f46e5),
            }
        };
    }
    let palette = theme.orb(dark);
    let (canvas, accent, action) = (
        palette.canvas.rgb(),
        palette.accent.rgb(),
        palette.action.rgb(),
    );
    let (white, black) = ([1.; 3], [0.; 3]);
    let tinted = |weight: f32| color(mix(canvas, accent, weight), 1.);
    if dark {
        Wire {
            canvas: color(canvas, 1.),
            sidebar: color(mix(canvas, white, 0.98), 1.),
            surface: color(mix(canvas, white, 0.96), 1.),
            accent_surface: tinted(0.8),
            message_surface: tinted(0.7),
            action: color(action, 1.),
        }
    } else {
        Wire {
            canvas: color(canvas, 1.),
            sidebar: color(mix(canvas, black, 0.98), 1.),
            surface: color(mix(canvas, white, 0.2), 1.),
            accent_surface: tinted(0.88),
            message_surface: tinted(0.8),
            action: color(action, 1.),
        }
    }
}

fn bit(left: f32, top: f32, width: f32, height: f32) -> gpui::Div {
    div()
        .absolute()
        .left(relative(left))
        .top(relative(top))
        .w(relative(width))
        .h(relative(height))
}

/// One pane of `ThemeWireframePane`, every box placed by percent of the pane.
fn pane(wire: &Wire) -> gpui::Div {
    let line: Hsla = Rgba {
        r: 127. / 255.,
        g: 127. / 255.,
        b: 127. / 255.,
        a: 0.25,
    }
    .into();
    let (sm, md, lg) = (px(4.), px(6.), px(8.));
    let card_shadow = vec![BoxShadow {
        color: gpui::black().opacity(0.14),
        offset: point(px(0.), px(2.)),
        blur_radius: px(5.),
        spread_radius: px(0.),
        inset: false,
    }];
    let rows = (0..3).map(|row| {
        let dot = match row {
            0 => rgb(0x34d399).into(),
            1 => wire.action,
            _ => rgb(0xfbbf24).into(),
        };
        div()
            .absolute()
            .left(relative(0.11))
            .right(relative(0.11))
            .top(relative(0.10 + 0.30 * row as f32))
            .h(relative(0.20))
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .size(px(3.5))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(dot)
                    .opacity(0.55),
            )
            .child(div().h(px(4.)).w(relative(0.52)).rounded(sm).bg(line))
    });
    div()
        .relative()
        .size_full()
        .bg(wire.canvas)
        .child(
            bit(0., 0., 0.22, 1.)
                .bg(wire.sidebar)
                .border_r_1()
                .border_color(line),
        )
        .child(
            bit(0.03, 0.08, 0.16, 0.08)
                .rounded(md)
                .bg(wire.surface)
                .border_1()
                .border_color(line),
        )
        .child(
            bit(0.03, 0.22, 0.16, 0.07)
                .rounded(md)
                .bg(wire.accent_surface),
        )
        .child(
            bit(0.03, 0.32, 0.16, 0.07)
                .rounded(md)
                .bg(wire.message_surface)
                .opacity(0.7),
        )
        .child(
            bit(0.03, 0.42, 0.16, 0.07)
                .rounded(md)
                .bg(wire.message_surface)
                .opacity(0.5),
        )
        .child(
            bit(0.48, 0.11, 0.24, 0.09)
                .rounded(lg)
                .bg(wire.message_surface),
        )
        .child(bit(0.27, 0.28, 0.34, 0.05).rounded(sm).bg(line))
        .child(bit(0.27, 0.38, 0.26, 0.05).rounded(sm).bg(line))
        .child(
            bit(0.26, 0.77, 0.68, 0.15)
                .flex()
                .items_center()
                .justify_between()
                .px_1()
                .rounded(md)
                .bg(wire.surface)
                .border_1()
                .border_color(line)
                .child(
                    div()
                        .h(relative(0.26))
                        .w(relative(0.34))
                        .rounded_full()
                        .bg(line)
                        .opacity(0.7),
                )
                .child(div().size(px(11.)).rounded_full().bg(wire.action)),
        )
        .child(
            bit(0.75, 0.08, 0.20, 0.46)
                .rounded(lg)
                .bg(wire.surface)
                .border_1()
                .border_color(line)
                .shadow(card_shadow)
                .children(rows),
        )
}

/// `ThemeWireframe` at `h-[8.75rem]`: one pane, or the light and dark panes split at the middle
/// for System, each clipped 1px short of the centre line.
pub(super) fn wireframe(mode: ColorMode, choice: ThemeChoice, cx: &App) -> gpui::Div {
    let border = Tokens::of(cx).border.opacity(0.6);
    let frame = div()
        .relative()
        .w_full()
        .h(px(140.))
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(border);
    let half = |left: bool, wire: &Wire| {
        let clipper = div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.5))
            .overflow_hidden();
        let inner = div().absolute().top_0().h_full().w(relative(2.));
        let clipper = if left {
            clipper.left_0()
        } else {
            clipper.right_0()
        };
        let inner = if left {
            inner.left_0()
        } else {
            inner.right_0()
        };
        clipper.child(inner.child(pane(wire)))
    };
    match mode {
        ColorMode::System => frame
            .child(half(true, &wire(choice.light, false)))
            .child(half(false, &wire(choice.dark, true))),
        ColorMode::Light => frame.child(pane(&wire(choice.light, false))),
        ColorMode::Dark => frame.child(pane(&wire(choice.dark, true))),
    }
}

/// Alpha of a gradient with `(offset, alpha)` stops at distance fraction `t`.
fn ramp(stops: &[(f32, f32)], t: f32) -> f32 {
    let mut previous = stops[0];
    for &stop in stops {
        if t <= stop.0 {
            let span = (stop.0 - previous.0).max(f32::EPSILON);
            return previous.1 + (stop.1 - previous.1) * ((t - previous.0) / span).clamp(0., 1.);
        }
        previous = stop;
    }
    previous.1
}

/// One radial gradient as concentric circles, outermost first.
fn gradient(
    rgb: [f32; 3],
    center: (f32, f32),
    stops: &[(f32, f32)],
    steps: usize,
    size: f32,
) -> Vec<gpui::Div> {
    let (cx, cy) = (center.0 * size, center.1 * size);
    let reach = [(0., 0.), (size, 0.), (0., size), (size, size)]
        .iter()
        .map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
        .fold(0., f32::max);
    let end = stops[stops.len() - 1].0;
    let mut circles = Vec::new();
    let mut outer = 0.;
    for step in 0..steps {
        let radius = reach * end * (1. - step as f32 / steps as f32);
        let next = reach * end * (1. - (step + 1) as f32 / steps as f32);
        let t = ((radius + next) / 2.) / reach;
        let total = ramp(stops, t).min(0.98);
        let alpha = (1. - (1. - total) / (1. - outer)).max(0.);
        outer = outer + alpha * (1. - outer);
        circles.push(
            div()
                .absolute()
                .left(px(cx - radius))
                .top(px(cy - radius))
                .size(px(radius * 2.))
                .rounded_full()
                .bg(color(rgb, alpha)),
        );
    }
    circles
}

/// The 56px orb (`ThemePreviewCircles`): the canvas tinted to 80% of the way to the page color,
/// the accent glowing from one corner and the message action from the opposite one.
pub(super) fn orb(palette: OrbPalette, dark: bool, cx: &App) -> gpui::Div {
    const INNER: f32 = 52.;
    let theme = Tokens::of(cx);
    let target = if dark {
        [9. / 255., 9. / 255., 11. / 255.]
    } else {
        [1.; 3]
    };
    let base = mix(palette.canvas.rgb(), target, 0.8);
    let accent = palette.accent.rgb();
    let action = palette.action.rgb();
    let (accent_stops, accent_center, action_center) = if dark {
        (
            [(0., 1.), (0.28, 0.62), (0.58, 0.)],
            (0.28, 0.78),
            (0.82, 0.18),
        )
    } else {
        (
            [(0., 1.), (0.28, 0.72), (0.58, 0.)],
            (0.72, 0.22),
            (0.18, 0.82),
        )
    };
    let shadow = BoxShadow {
        color: gpui::black().opacity(if dark { 0.18 } else { 0.08 }),
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: false,
    };
    div()
        .relative()
        .size(px(56.))
        .flex_shrink_0()
        .overflow_hidden()
        .rounded_full()
        .border_2()
        .border_color(theme.background)
        .bg(color(base, 1.))
        .shadow(vec![shadow])
        .children(gradient(accent, accent_center, &accent_stops, 12, INNER))
        .children(gradient(
            action,
            action_center,
            &[(0., 0.45), (0.55, 0.)],
            6,
            INNER,
        ))
}

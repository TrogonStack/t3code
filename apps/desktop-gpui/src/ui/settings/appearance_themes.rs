//! The wireframe and orb drawings of the Appearance tab: `ThemeWireframe.tsx` and
//! `ThemePreviewCircles.tsx`. gpui has no radial gradients or blur, so each orb is rasterized on
//! the CPU once per theme, mode and device scale and drawn as an image.

use super::*;
use crate::settings_appearance::{ColorMode, Paint, ThemeChoice, ThemeId, mix_oklab};
use gpui::{BoxShadow, Hsla, ImageSource, RenderImage, Rgba, img, point, relative, rgb};
use image::{Frame, RgbaImage};
use smallvec::smallvec;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

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

fn wire(theme: ThemeId, dark: bool) -> Wire {
    let (palette, roles) = (theme.orb(dark), theme.roles(dark));
    let paint = |paint: Paint| color(paint.rgb(), 1.);
    Wire {
        canvas: paint(palette.canvas),
        sidebar: paint(roles.sidebar),
        surface: paint(roles.surface),
        accent_surface: paint(roles.accent_surface),
        message_surface: paint(roles.message_surface),
        action: paint(palette.action),
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

/// The orb's inner box and clip circle, in logical pixels (`size-14` minus the 2px border).
const INNER: f32 = 52.;
const RADIUS: f32 = INNER / 2.;
/// `THEME_PREVIEW_RENDER_SPECS`: blur sigma and the scale applied after the blur.
const BLUR: f32 = 3.;
const ZOOM: f32 = 1.1;

/// One radial gradient layer: a constant color whose alpha follows `(offset, alpha)` stops by the
/// distance from `center` as a fraction of the farthest-corner reach.
struct Radial {
    color: [f32; 3],
    center: (f32, f32),
    stops: [(f32, f32); 3],
}

impl Radial {
    fn alpha(&self, x: f32, y: f32) -> f32 {
        let (cx, cy) = (self.center.0 * INNER, self.center.1 * INNER);
        let reach = [(0., 0.), (INNER, 0.), (0., INNER), (INNER, INNER)]
            .iter()
            .map(|(px, py)| ((px - cx).powi(2) + (py - cy).powi(2)).sqrt())
            .fold(0., f32::max);
        let t = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / reach;
        let mut previous = self.stops[0];
        for stop in self.stops {
            if t <= stop.0 {
                let span = (stop.0 - previous.0).max(f32::EPSILON);
                return previous.1
                    + (stop.1 - previous.1) * ((t - previous.0) / span).clamp(0., 1.);
            }
            previous = stop;
        }
        previous.1
    }
}

/// Coverage of a pixel whose center lies `distance` logical pixels inside an edge.
fn coverage(distance: f32, scale: f32) -> f32 {
    (distance * scale + 0.5).clamp(0., 1.)
}

fn noise(x: usize, y: usize) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65536.
}

/// A separable Gaussian blur of premultiplied pixels; outside the buffer is transparent.
fn blur(pixels: &mut [[f32; 4]], size: usize, sigma: f32) {
    let radius = (sigma * 3.).ceil() as isize;
    let mut kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-((i * i) as f32) / (2. * sigma * sigma)).exp())
        .collect();
    let total: f32 = kernel.iter().sum();
    kernel.iter_mut().for_each(|w| *w /= total);
    let mut scratch = vec![[0.; 4]; pixels.len()];
    for horizontal in [true, false] {
        for y in 0..size {
            for x in 0..size {
                let mut sum = [0.; 4];
                for (tap, weight) in kernel.iter().enumerate() {
                    let offset = tap as isize - radius;
                    let (sx, sy) = if horizontal {
                        (x as isize + offset, y as isize)
                    } else {
                        (x as isize, y as isize + offset)
                    };
                    if (0..size as isize).contains(&sx) && (0..size as isize).contains(&sy) {
                        let source = pixels[sy as usize * size + sx as usize];
                        for channel in 0..4 {
                            sum[channel] += source[channel] * weight;
                        }
                    }
                }
                scratch[y * size + x] = sum;
            }
        }
        pixels.copy_from_slice(&scratch);
    }
}

fn sample(pixels: &[[f32; 4]], size: usize, x: f32, y: f32) -> [f32; 4] {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: isize, dy: isize| {
        let (px, py) = (x0 as isize + dx, y0 as isize + dy);
        if (0..size as isize).contains(&px) && (0..size as isize).contains(&py) {
            pixels[py as usize * size + px as usize]
        } else {
            [0.; 4]
        }
    };
    let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
    std::array::from_fn(|i| {
        (a[i] * (1. - fx) + b[i] * fx) * (1. - fy) + (c[i] * (1. - fx) + d[i] * fx) * fy
    })
}

/// `ThemePreviewCircle` as straight RGBA: the oklab-tinted canvas under the accent and action
/// radial layers, clipped to the circle, blurred, zoomed about the center, then clipped again
/// with the 1px edge ring showing through the blurred rim.
fn rasterize(theme: ThemeId, dark: bool, scale: u32) -> Vec<u8> {
    let palette = theme.orb(dark);
    let target = if dark {
        [9. / 255., 9. / 255., 11. / 255.]
    } else {
        [1.; 3]
    };
    let base = mix_oklab(palette.canvas.rgb(), target, 0.8);
    let (accent, action) = if dark {
        (
            Radial {
                color: palette.accent.rgb(),
                center: (0.28, 0.78),
                stops: [(0., 1.), (0.28, 0.62), (0.58, 0.)],
            },
            Radial {
                color: palette.action.rgb(),
                center: (0.82, 0.18),
                stops: [(0., 0.45), (0.55, 0.), (0.55, 0.)],
            },
        )
    } else {
        (
            Radial {
                color: palette.accent.rgb(),
                center: (0.72, 0.22),
                stops: [(0., 1.), (0.28, 0.72), (0.58, 0.)],
            },
            Radial {
                color: palette.action.rgb(),
                center: (0.18, 0.82),
                stops: [(0., 0.45), (0.55, 0.), (0.55, 0.)],
            },
        )
    };
    let s = scale as f32;
    let size = (INNER * s) as usize;
    let local = |index: usize| (index as f32 + 0.5) / s;
    let radius_at = |x: f32, y: f32| ((x - RADIUS).powi(2) + (y - RADIUS).powi(2)).sqrt();

    let mut layer = vec![[0.; 4]; size * size];
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (local(x), local(y));
            let inside = coverage(RADIUS - radius_at(u, v), s);
            let mut rgb = base;
            for radial in [&action, &accent] {
                rgb = mix_rgb(rgb, radial.color, radial.alpha(u, v));
            }
            layer[y * size + x] = [rgb[0] * inside, rgb[1] * inside, rgb[2] * inside, inside];
        }
    }
    blur(&mut layer, size, BLUR * s);

    let (ring, ring_alpha) = if dark { (1., 0.14) } else { (0., 0.10) };
    let mut out = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (local(x), local(y));
            let radius = radius_at(u, v);
            let from = |p: f32| (RADIUS + (p - RADIUS) / ZOOM) * s - 0.5;
            let zoomed = sample(&layer, size, from(u), from(v));
            let edge = coverage(RADIUS - radius, s) * (1. - coverage(RADIUS - 1. - radius, s));
            let under = ring_alpha * edge;
            let alpha = zoomed[3] + under * (1. - zoomed[3]);
            let premultiplied: [f32; 3] =
                std::array::from_fn(|i| zoomed[i] + ring * under * (1. - zoomed[3]));
            let clip = coverage(RADIUS - radius, s);
            let final_alpha = alpha * clip;
            let jitter = noise(x, y) - 0.5;
            let byte = |value: f32| (value * 255. + jitter).round().clamp(0., 255.) as u8;
            let straight = |value: f32| if alpha > 0. { value / alpha } else { 0. };
            out.extend([
                byte(straight(premultiplied[0])),
                byte(straight(premultiplied[1])),
                byte(straight(premultiplied[2])),
                (final_alpha * 255.).round().clamp(0., 255.) as u8,
            ]);
        }
    }
    out
}

fn mix_rgb(from: [f32; 3], to: [f32; 3], alpha: f32) -> [f32; 3] {
    std::array::from_fn(|i| from[i] * (1. - alpha) + to[i] * alpha)
}

type OrbKey = (ThemeId, bool, u32);

fn orb_image(theme: ThemeId, dark: bool, scale: u32) -> Arc<RenderImage> {
    static CACHE: OnceLock<Mutex<HashMap<OrbKey, Arc<RenderImage>>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry((theme, dark, scale))
        .or_insert_with(|| {
            let side = INNER as u32 * scale;
            let mut pixels = rasterize(theme, dark, scale);
            // gpui samples BGRA.
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let image = RgbaImage::from_raw(side, side, pixels).expect("square RGBA buffer");
            Arc::new(RenderImage::new(smallvec![Frame::new(image)]))
        })
        .clone()
}

/// The 56px orb: the rasterized `ThemePreviewCircle` inside its 2px page-colored border and drop
/// shadow. The image is already clipped to the circle, so nothing paints outside it.
pub(super) fn orb(theme: ThemeId, dark: bool, scale: u32, cx: &App) -> gpui::Div {
    let shadow = BoxShadow {
        color: gpui::black().opacity(if dark { 0.18 } else { 0.08 }),
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: false,
    };
    div()
        .size(px(56.))
        .flex_shrink_0()
        .rounded_full()
        .border_2()
        .border_color(Tokens::of(cx).background)
        .shadow(vec![shadow])
        .child(img(ImageSource::Render(orb_image(theme, dark, scale))).size(px(INNER)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(pixels: &[u8], size: usize, x: usize, y: usize) -> u8 {
        pixels[(y * size + x) * 4 + 3]
    }

    #[test]
    fn orb_is_clipped_to_the_circle() {
        for theme in ThemeId::ALL {
            for dark in [false, true] {
                let pixels = rasterize(theme, dark, 2);
                let size = 104;
                for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
                    assert_eq!(alpha_at(&pixels, size, x, y), 0, "{theme:?} corner");
                }
                assert_eq!(alpha_at(&pixels, size, 52, 52), 255, "{theme:?} center");
                assert!(alpha_at(&pixels, size, 52, 0) < 255, "{theme:?} rim");
            }
        }
    }
}

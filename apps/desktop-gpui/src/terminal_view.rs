//! The terminal drawer's surface, ported from `ThreadTerminalDrawer.tsx`: the shell's screen
//! drawn as text runs, with the keyboard, paste and wheel wired to the session.

use crate::connection::{Action, ActionSender};
use crate::terminal::{self, Event, FALLBACK_GRID, Grid};
use crate::terminal_screen::{Palette, Row, Screen, Span, mark_cursor};
use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, Font, FontStyle, FontWeight, Hsla, KeyDownEvent,
    MouseButton, Pixels, Render, Rgba, ScrollEvent, StrikethroughStyle, StyledText, TextRun,
    UnderlineStyle, Window, canvas, div, prelude::*, px,
};
use gpui_component::ActiveTheme;
use std::cell::Cell;
use std::rc::Rc;

/// The drawer's inner padding around the character grid.
const PADDING_X: f32 = 12.;
const PADDING_Y: f32 = 8.;
/// Line height as a multiple of the mono font size.
const LINE_HEIGHT_RATIO: f32 = 1.35;

pub struct TerminalView {
    thread_id: Option<String>,
    screen: Screen,
    /// Whether the server's snapshot has landed; input and resizes wait for it.
    ready: bool,
    /// Whether the shell is gone, so there is nothing to type into.
    ended: bool,
    /// The grid last sent to the server for this attachment.
    requested: Option<Grid>,
    focus: FocusHandle,
    /// The grid area's bounds at the last paint, for fitting the character grid.
    area: Rc<Cell<Bounds<Pixels>>>,
    /// Wheel movement short of a whole line.
    scroll_remainder: f32,
    actions: ActionSender,
}

impl TerminalView {
    pub fn new(actions: ActionSender, cx: &mut Context<Self>) -> Self {
        Self {
            thread_id: None,
            screen: Screen::new(FALLBACK_GRID),
            ready: false,
            ended: false,
            requested: None,
            focus: cx.focus_handle(),
            area: Rc::default(),
            scroll_remainder: 0.,
            actions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    /// Points the surface at a thread's terminal; the server replays the session's history.
    pub fn show(&mut self, thread_id: Option<String>) {
        if self.thread_id == thread_id {
            return;
        }
        self.thread_id = thread_id;
        self.screen = Screen::new(FALLBACK_GRID);
        self.ready = false;
        self.ended = false;
        self.requested = None;
        self.scroll_remainder = 0.;
    }

    /// Applies one element of the attach stream; an element for another thread is stale.
    pub fn apply(&mut self, thread_id: &str, event: Event, cx: &mut Context<Self>) {
        if self.thread_id.as_deref() != Some(thread_id) {
            return;
        }
        match event {
            Event::Snapshot {
                history,
                grid,
                exited,
            } => {
                let replies = self.screen.reset(&history, grid.unwrap_or(FALLBACK_GRID));
                self.ready = true;
                self.ended = false;
                self.requested = None;
                self.reply(replies);
                if exited {
                    self.end("Process exited");
                }
            }
            Event::Restarted { history, grid } => {
                let replies = self.screen.reset(&history, grid.unwrap_or(FALLBACK_GRID));
                self.ended = false;
                self.requested = None;
                self.reply(replies);
            }
            Event::Output(data) => {
                let replies = self.screen.feed(&data);
                self.reply(replies);
            }
            Event::Cleared => {
                self.screen.feed("\x1b[2J\x1b[3J\x1b[H");
            }
            Event::Resized(grid) => self.screen.resize(grid),
            Event::Exited => self.end("Process exited"),
            Event::Closed => self.end("Terminal closed"),
            Event::Error(message) => self.end(&format!("Terminal error: {message}")),
        }
        cx.notify();
    }

    /// Prints why the shell is gone, and releases the server's session as the web does on exit.
    fn end(&mut self, message: &str) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.screen
            .feed(&format!("\r\n\x1b[2m[{message}]\x1b[0m\r\n"));
        if let Some(thread_id) = self.thread_id.clone() {
            self.send(Action::TerminalClose { thread_id });
        }
    }

    fn send(&self, action: Action) {
        if self.actions.try_send(action).is_err() {
            eprintln!("[ui] could not reach the terminal");
        }
    }

    fn write(&mut self, data: String) {
        let Some(thread_id) = self.thread_id.clone().filter(|_| self.ready && !self.ended) else {
            return;
        };
        self.screen.snap_to_bottom();
        self.send(Action::TerminalWrite { thread_id, data });
    }

    /// What the shell's terminal queries asked to be written back.
    fn reply(&mut self, replies: Vec<String>) {
        for reply in replies {
            self.write(reply);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform && keystroke.key == "v" {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            if let Some(text) = text.filter(|text| !text.is_empty()) {
                self.write(terminal::paste_text(&text, self.screen.bracketed_paste()));
            }
            cx.stop_propagation();
            return;
        }
        if let Some(data) = terminal::encode_key(keystroke, self.screen.application_cursor()) {
            self.write(data);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn scroll(&mut self, event: &ScrollEvent, line_height: f32, cx: &mut Context<Self>) {
        if self.screen.alternate_screen() {
            return;
        }
        let moved = f32::from(event.delta.pixel_delta(px(line_height)).y) / line_height;
        self.scroll_remainder += moved;
        let lines = self.scroll_remainder.trunc();
        if lines != 0. {
            self.scroll_remainder -= lines;
            self.screen.scroll(lines as i32);
            cx.notify();
        }
        cx.stop_propagation();
    }

    /// Fits the grid to the painted area and tells the server when it changed.
    fn fit(&mut self, cell_width: f32, line_height: f32) {
        let area = self.area.get();
        let Some(grid) = Grid::fit(
            f32::from(area.size.width),
            f32::from(area.size.height),
            cell_width,
            line_height,
        ) else {
            return;
        };
        if !self.ready || self.ended || self.requested == Some(grid) {
            return;
        }
        self.requested = Some(grid);
        self.screen.resize(grid);
        if let Some(thread_id) = self.thread_id.clone() {
            self.send(Action::TerminalResize { thread_id, grid });
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn rgb_of(color: Hsla) -> Rgb {
    let color = Rgba::from(color);
    let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    Rgb {
        r: byte(color.r),
        g: byte(color.g),
        b: byte(color.b),
    }
}

fn hsla_of(color: Rgb) -> Hsla {
    gpui::rgb((color.r as u32) << 16 | (color.g as u32) << 8 | color.b as u32).into()
}

fn runs(spans: &[Span], font: &Font) -> Vec<TextRun> {
    spans
        .iter()
        .map(|span| {
            let style = span.style;
            let color = hsla_of(style.foreground);
            TextRun {
                len: span.len,
                font: Font {
                    weight: if style.bold {
                        FontWeight::BOLD
                    } else {
                        font.weight
                    },
                    style: if style.italic {
                        FontStyle::Italic
                    } else {
                        font.style
                    },
                    ..font.clone()
                },
                color,
                background_color: style.background.map(hsla_of),
                underline: style.underline.then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: Some(color),
                    wavy: false,
                }),
                strikethrough: style.strikethrough.then(|| StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(color),
                }),
            }
        })
        .collect()
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let family = theme.mono_font_family.clone();
        let size = theme.mono_font_size;
        let background = theme.background;
        let palette = Palette::dark(rgb_of(theme.foreground), rgb_of(background));
        let font = gpui::font(family.clone());
        let line_height = (f32::from(size) * LINE_HEIGHT_RATIO).round();
        let cell_width = {
            let text_system = window.text_system();
            let font_id = text_system.resolve_font(&font);
            text_system
                .advance(font_id, size, 'm')
                .map_or(f32::from(size) * 0.6, |advance| f32::from(advance.width))
        };
        self.fit(cell_width, line_height);

        let focused = self.focus.is_focused(window);
        let (rows, cursor) = self.screen.rows(&palette);
        let cursor_colors = if focused {
            (palette.background, palette.foreground)
        } else {
            (palette.foreground, palette.foreground)
        };
        let lines: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(index, row): (usize, &Row)| {
                let spans = match cursor.filter(|(line, _)| *line == index) {
                    Some((_, column)) if focused => {
                        mark_cursor(row, column, cursor_colors.0, cursor_colors.1)
                    }
                    Some((_, column)) => {
                        // Unfocused, the cursor is a dim block rather than a solid one.
                        let dim = Rgb {
                            r: (palette.foreground.r as u16 / 2) as u8,
                            g: (palette.foreground.g as u16 / 2) as u8,
                            b: (palette.foreground.b as u16 / 2) as u8,
                        };
                        mark_cursor(row, column, palette.foreground, dim)
                    }
                    None => row.spans.clone(),
                };
                div()
                    .h(px(line_height))
                    .flex_none()
                    .whitespace_nowrap()
                    .child(StyledText::new(row.text.clone()).with_runs(runs(&spans, &font)))
            })
            .collect();

        let measured = self.area.clone();
        div()
            .id("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .overflow_hidden()
            .bg(background)
            .font_family(family)
            .text_size(size)
            .line_height(px(line_height))
            .text_color(theme.foreground)
            .px(px(PADDING_X))
            .py(px(PADDING_Y))
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus(window, cx)),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key_down(event, cx)))
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                this.scroll(event, line_height, cx)
            }))
            .child(
                div()
                    .relative()
                    .size_full()
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if measured.replace(bounds) != bounds {
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children(lines),
            )
    }
}

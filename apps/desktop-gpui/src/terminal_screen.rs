//! The terminal's screen: the server's output parsed into a character grid, and that grid read
//! back as runs of styled text. The web feeds the same bytes to xterm.js in
//! `ThreadTerminalDrawer.tsx`; this is the alacritty parser doing that job.

use crate::terminal::Grid;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};
use std::cell::RefCell;
use std::rc::Rc;

/// The web's `scrollback` option for the drawer's xterm.
const SCROLLBACK: usize = 5_000;

/// The sixteen ANSI colors plus the default foreground and background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub ansi: [Rgb; 16],
}

const fn rgb(hex: u32) -> Rgb {
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

impl Palette {
    /// The ANSI colors of a dark theme; the window's theme supplies the default pair.
    pub const fn dark(foreground: Rgb, background: Rgb) -> Self {
        Self {
            foreground,
            background,
            ansi: [
                rgb(0x3b4252),
                rgb(0xe06c75),
                rgb(0x98c379),
                rgb(0xe5c07b),
                rgb(0x61afef),
                rgb(0xc678dd),
                rgb(0x56b6c2),
                rgb(0xabb2bf),
                rgb(0x5c6370),
                rgb(0xff7b86),
                rgb(0xb1e18b),
                rgb(0xf0d197),
                rgb(0x7bc0ff),
                rgb(0xd88cf0),
                rgb(0x6fd0dc),
                rgb(0xffffff),
            ],
        }
    }

    fn indexed(&self, index: usize) -> Rgb {
        match index {
            0..=15 => self.ansi[index],
            16..=231 => {
                let cube = index - 16;
                let level = |step: usize| if step == 0 { 0 } else { (55 + 40 * step) as u8 };
                Rgb {
                    r: level(cube / 36),
                    g: level(cube / 6 % 6),
                    b: level(cube % 6),
                }
            }
            _ => {
                let gray = (8 + 10 * (index - 232)) as u8;
                Rgb {
                    r: gray,
                    g: gray,
                    b: gray,
                }
            }
        }
    }

    fn resolve(&self, color: Color, overrides: &alacritty_terminal::term::color::Colors) -> Rgb {
        let index = match color {
            Color::Spec(rgb) => return rgb,
            Color::Indexed(index) => index as usize,
            Color::Named(name) => name as usize,
        };
        if let Some(rgb) = overrides[index] {
            return rgb;
        }
        match color {
            Color::Indexed(index) => self.indexed(index as usize),
            Color::Named(NamedColor::Background | NamedColor::Cursor) => self.background,
            Color::Named(name) => {
                let index = name as usize;
                match index {
                    0..=15 => self.ansi[index],
                    // DimBlack = 259 through DimWhite = 266, then the foreground pair.
                    259..=266 => dim(self.ansi[index - 259]),
                    268 => dim(self.foreground),
                    _ => self.foreground,
                }
            }
            Color::Spec(rgb) => rgb,
        }
    }
}

fn dim(color: Rgb) -> Rgb {
    let scale = |value: u8| (value as u16 * 2 / 3) as u8;
    Rgb {
        r: scale(color.r),
        g: scale(color.g),
        b: scale(color.b),
    }
}

/// How one run of cells draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub foreground: Rgb,
    /// `None` is the default background, which the drawer's own fill already paints.
    pub background: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

/// `len` bytes of a row's text drawn in `style`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub len: usize,
    pub style: Style,
}

/// One screen row: always `cols` cells of text, so a column is a character offset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub spans: Vec<Span>,
}

/// What the shell asked the terminal to say back (cursor position reports, color queries).
#[derive(Clone, Default)]
struct Replies(Rc<RefCell<Vec<String>>>);

impl EventListener for Replies {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            self.0.borrow_mut().push(text);
        }
    }
}

struct Size(Grid);

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.0.rows
    }

    fn screen_lines(&self) -> usize {
        self.0.rows
    }

    fn columns(&self) -> usize {
        self.0.cols
    }
}

pub struct Screen {
    term: Term<Replies>,
    parser: Processor,
    replies: Replies,
    grid: Grid,
}

impl Screen {
    pub fn new(grid: Grid) -> Self {
        let replies = Replies::default();
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &Size(grid), replies.clone()),
            parser: Processor::new(),
            replies,
            grid,
        }
    }

    #[cfg(test)]
    fn grid(&self) -> Grid {
        self.grid
    }

    /// Parses output; the result is what the shell's terminal queries need written back.
    pub fn feed(&mut self, data: &str) -> Vec<String> {
        self.parser.advance(&mut self.term, data.as_bytes());
        self.replies.0.take()
    }

    /// Starts over from the server's copy of the session.
    pub fn reset(&mut self, history: &str, grid: Grid) -> Vec<String> {
        *self = Self::new(grid);
        self.feed(history)
    }

    pub fn resize(&mut self, grid: Grid) {
        if grid != self.grid {
            self.grid = grid;
            self.term.resize(Size(grid));
        }
    }

    /// Scrolls the view into the history by `lines` (positive is up); output snaps it back.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn snap_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    pub fn application_cursor(&self) -> bool {
        self.term.mode().contains(TermMode::APP_CURSOR)
    }

    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Whether a full-screen program owns the screen, where the wheel has no scrollback to move.
    pub fn alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// The visible rows, and the cursor's row and column when it shows.
    pub fn rows(&self, palette: &Palette) -> (Vec<Row>, Option<(usize, usize)>) {
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let mut rows = vec![Row::default(); self.grid.rows];
        for cell in content.display_iter {
            let flags = cell.cell.flags;
            if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let Some(row) = usize::try_from(cell.point.line.0 + offset)
                .ok()
                .and_then(|row| rows.get_mut(row))
            else {
                continue;
            };
            let mut foreground = palette.resolve(cell.cell.fg, content.colors);
            let mut background = palette.resolve(cell.cell.bg, content.colors);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if flags.contains(Flags::DIM) {
                foreground = dim(foreground);
            }
            if flags.contains(Flags::HIDDEN) {
                foreground = background;
            }
            let style = Style {
                foreground,
                background: (background != palette.background).then_some(background),
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strikethrough: flags.contains(Flags::STRIKEOUT),
            };
            push(row, cell.cell.c, style);
        }
        let cursor = content.cursor;
        let cursor = (cursor.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden
            && content.display_offset == 0)
            .then_some((cursor.point.line.0, cursor.point.column.0))
            .and_then(|(line, column)| Some((usize::try_from(line).ok()?, column)));
        (rows, cursor)
    }

    #[cfg(test)]
    fn text(&self) -> Vec<String> {
        let palette = Palette::dark(rgb(0xffffff), rgb(0));
        let (rows, _) = self.rows(&palette);
        rows.into_iter()
            .map(|row| row.text.trim_end().to_owned())
            .collect()
    }
}

/// The row's spans with the cell at `column` drawn as the cursor: `foreground` on `background`.
pub fn mark_cursor(row: &Row, column: usize, foreground: Rgb, background: Rgb) -> Vec<Span> {
    let Some((start, c)) = row.text.char_indices().nth(column) else {
        return row.spans.clone();
    };
    let end = start + c.len_utf8();
    let mut marked = Vec::with_capacity(row.spans.len() + 2);
    let mut offset = 0;
    for span in &row.spans {
        let (from, to) = (offset, offset + span.len);
        offset = to;
        if to <= start || from >= end {
            marked.push(span.clone());
            continue;
        }
        if from < start {
            marked.push(Span {
                len: start - from,
                style: span.style,
            });
        }
        marked.push(Span {
            len: end - start,
            style: Style {
                foreground,
                background: Some(background),
                ..span.style
            },
        });
        if to > end {
            marked.push(Span {
                len: to - end,
                style: span.style,
            });
        }
    }
    marked
}

fn push(row: &mut Row, c: char, style: Style) {
    row.text.push(c);
    let len = c.len_utf8();
    match row.spans.last_mut() {
        Some(last) if last.style == style => last.len += len,
        _ => row.spans.push(Span { len, style }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid { cols: 12, rows: 4 };

    fn palette() -> Palette {
        Palette::dark(rgb(0xeeeeee), rgb(0x111111))
    }

    #[test]
    fn output_lands_on_the_grid() {
        let mut screen = Screen::new(GRID);
        screen.feed("hello\r\nworld");
        assert_eq!(screen.text(), ["hello", "world", "", ""]);
        screen.feed("\x1b[2J\x1b[Hagain");
        assert_eq!(screen.text(), ["again", "", "", ""]);
    }

    #[test]
    fn a_row_is_always_as_wide_as_the_grid() {
        let screen = Screen::new(GRID);
        let (rows, _) = screen.rows(&palette());
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.text.chars().count() == 12));
        assert!(rows.iter().all(|row| row.spans.len() == 1));
    }

    #[test]
    fn colors_and_attributes_split_the_runs() {
        let mut screen = Screen::new(GRID);
        screen.feed("a\x1b[1;31mb\x1b[0m\x1b[44mc\x1b[7md\x1b[0m");
        let (rows, _) = screen.rows(&palette());
        let spans = &rows[0].spans;
        let palette = palette();
        assert_eq!(spans[0].len, 1);
        assert_eq!(spans[0].style.foreground, palette.foreground);
        assert!(!spans[0].style.bold);
        assert_eq!(spans[1].style.foreground, palette.ansi[1]);
        assert!(spans[1].style.bold);
        assert_eq!(spans[2].style.background, Some(palette.ansi[4]));
        // Inverse swaps the pair, so the default background becomes the text color.
        assert_eq!(spans[3].style.foreground, palette.ansi[4]);
        assert_eq!(spans[3].style.background, Some(palette.foreground));
        assert_eq!(spans.iter().map(|span| span.len).sum::<usize>(), 12);
    }

    #[test]
    fn extended_colors_resolve() {
        let mut screen = Screen::new(GRID);
        screen.feed("\x1b[38;5;196ma\x1b[38;5;240mb\x1b[38;2;1;2;3mc");
        let (rows, _) = screen.rows(&palette());
        let colors: Vec<_> = rows[0]
            .spans
            .iter()
            .take(3)
            .map(|span| span.style.foreground)
            .collect();
        assert_eq!(colors[0], rgb(0xff0000));
        assert_eq!(colors[1], rgb(0x585858));
        assert_eq!(colors[2], rgb(0x010203));
    }

    #[test]
    fn the_cursor_follows_output_and_hides_on_request() {
        let mut screen = Screen::new(GRID);
        screen.feed("ab\r\nc");
        assert_eq!(screen.rows(&palette()).1, Some((1, 1)));
        screen.feed("\x1b[?25l");
        assert_eq!(screen.rows(&palette()).1, None);
    }

    #[test]
    fn the_cursor_cell_splits_its_run() {
        let mut screen = Screen::new(GRID);
        screen.feed("abcd");
        let palette = palette();
        let (rows, _) = screen.rows(&palette);
        let marked = mark_cursor(&rows[0], 2, palette.background, palette.foreground);
        let lens: Vec<_> = marked.iter().map(|span| span.len).collect();
        assert_eq!(lens, [2, 1, 9]);
        assert_eq!(marked[1].style.background, Some(palette.foreground));
        assert_eq!(marked[0].style, rows[0].spans[0].style);
        assert_eq!(
            mark_cursor(&rows[0], 99, palette.background, palette.foreground),
            rows[0].spans
        );
    }

    #[test]
    fn terminal_queries_come_back_as_replies() {
        let mut screen = Screen::new(GRID);
        assert_eq!(screen.feed("ab\x1b[6n"), ["\x1b[1;3R"]);
        assert!(screen.feed("c").is_empty());
    }

    #[test]
    fn modes_follow_the_programs_requests() {
        let mut screen = Screen::new(GRID);
        assert!(!screen.application_cursor() && !screen.bracketed_paste());
        screen.feed("\x1b[?1h\x1b[?2004h");
        assert!(screen.application_cursor() && screen.bracketed_paste());
        screen.feed("\x1b[?1049h");
        assert!(screen.alternate_screen());
    }

    #[test]
    fn scrolling_reads_the_history() {
        let mut screen = Screen::new(GRID);
        screen.feed("1\r\n2\r\n3\r\n4\r\n5\r\n6");
        assert_eq!(screen.text(), ["3", "4", "5", "6"]);
        screen.scroll(2);
        assert_eq!(screen.text(), ["1", "2", "3", "4"]);
        assert_eq!(screen.rows(&palette()).1, None);
        screen.snap_to_bottom();
        assert_eq!(screen.text(), ["3", "4", "5", "6"]);
    }

    #[test]
    fn reset_replays_the_servers_history_and_resize_reflows() {
        let mut screen = Screen::new(GRID);
        screen.feed("stale");
        screen.reset("fresh\r\n", Grid { cols: 6, rows: 2 });
        assert_eq!(screen.text(), ["fresh", ""]);
        assert_eq!(screen.grid(), Grid { cols: 6, rows: 2 });
        screen.resize(Grid { cols: 20, rows: 3 });
        assert_eq!(screen.text().len(), 3);
        assert_eq!(screen.rows(&palette()).0[0].text.chars().count(), 20);
    }
}

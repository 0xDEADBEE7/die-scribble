//! die-scribble: a browser for a directory of markdown notes, as an extension of DIE that depends
//! on die-core alone. `scribble` shows `*scribble*` in this window: a title, the search line,
//! the columns and a row per note, newest first, drawn as seek draws scrib.el's browser in Emacs.
//! Typing filters the notes by name, and `RET` opens the selected one as an ordinary file.
//!
//! It's a read-only text buffer with two modes on it: [`MODE`], which holds the browser's state
//! and its keys, and [`SEARCH_MODE`] over it while the query is typed, whose `<char>` takes every
//! printable key. Every change draws the whole buffer again, and the faces come from the mode.
#![warn(missing_docs)]

use die_core::{
    Args, BufferId, Checkout, Ctx, Error, Extension, Mode, Origin, Registry, Result, ShowIn, State,
    Style, TextBuf, WindowId, Wrap,
};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The browser's buffer.
pub const BUFFER: &str = "*scribble*";
/// The browser's mode: its state and its keys.
pub const MODE: &str = "scribble-browser";
/// The mode searching adds over it, whose keys come first.
pub const SEARCH_MODE: &str = "scribble-search";
/// The search line's face while the query is typed, on `highlight`.
pub const SEARCH_FACE: &str = "scribble-search";
/// The notes' extension.
const EXTENSION: &str = "md";
/// The NAME column's width; a longer name is cut at one less, with `…`.
const NAME_WIDTH: usize = 42;
/// The most rows shown at once, as seek has it.
const MAX_VISIBLE: usize = 20;
/// The lines above the rows: the title, the search line, the columns and three rules.
const CHROME: usize = 6;

// ------------------------------------------------------------------ options

/// Your options.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Where the notes are. None is `$HOME/.config/scribble/`, read when `scribble` runs.
    pub directory: Option<PathBuf>,
    /// The title, centred in the top rule.
    pub title: String,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            directory: None,
            title: "scribble".into(),
        }
    }
}

impl Options {
    /// The notes' directory, made absolute.
    pub fn dir(&self) -> PathBuf {
        let dir = self.directory.clone().unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            Path::new(&home).join(".config/scribble")
        });
        std::path::absolute(&dir).unwrap_or(dir)
    }
}

// ------------------------------------------------------------------ notes

/// A note: its file's name without `.md`, and when the file last changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// The name.
    pub name: String,
    /// The file's modification time.
    pub modified: SystemTime,
}

/// The `.md` files in `dir`, hidden ones left out, newest first, then by name. None for a
/// directory that's missing or can't be read.
pub fn notes(dir: &Path) -> Vec<Note> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut notes: Vec<Note> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?;
            let md = path.extension().is_some_and(|x| x == EXTENSION);
            if !md || name.is_empty() || name.starts_with('.') {
                return None;
            }
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            Some(Note {
                name: name.to_string(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect();
    notes.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    notes
}

/// Whether every character of `query` is in `name`, in order, whatever the case.
pub fn fuzzy_match(query: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    let mut rest = name.chars();
    query
        .to_lowercase()
        .chars()
        .all(|q| rest.by_ref().any(|c| c == q))
}

/// How long ago `then` was: `just now`, `N minutes ago`, `N hours ago` or `N days ago`.
pub fn relative_time(then: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} minutes ago", secs / 60),
        3600..86400 => format!("{} hours ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}

/// A note's row: its name in the 42-column NAME, cut at 41 with `…`, a space, and UPDATED.
pub fn row(note: &Note, now: SystemTime) -> String {
    let mut name: String = note.name.chars().take(NAME_WIDTH - 1).collect();
    if name.len() < note.name.len() {
        name.push('…');
    }
    format!("{name:<NAME_WIDTH$} {}", relative_time(note.modified, now))
}

/// `text` with spaces after it, to `width` characters.
fn padded(text: &str, width: usize) -> String {
    let n = width.saturating_sub(text.chars().count());
    format!("{text}{}", " ".repeat(n))
}

// ------------------------------------------------------------------ the browser

/// The browser's state, in its buffer.
#[derive(Default)]
struct Browser {
    /// Where the notes are.
    dir: PathBuf,
    /// The title, with a space either side.
    title: String,
    /// The query.
    query: String,
    /// Whether the query is being typed, with [`Search`] on.
    searching: bool,
    /// The selected row, among those the query keeps.
    selected: usize,
    /// The first row shown.
    scroll: usize,
    /// The selected note's name, as last drawn.
    on: Option<String>,
    /// The faces of the text drawn last, and its revision.
    spans: Vec<(Range<usize>, &'static str)>,
    drawn: Option<u64>,
}

/// Searching: its keys come before the browser's.
struct Search;

impl Mode for Search {}

impl Mode for Browser {
    /// A window showing it changed size, or came to show it: draw it at that size.
    fn on_resize(&mut self, win: WindowId, _: usize, _: usize, ctx: &mut Ctx) {
        if let Some(buf) = ctx.window_buffer(win) {
            ctx.defer(move |ctx| redraw(ctx, buf));
        }
    }

    fn decorate(
        &mut self,
        text: &TextBuf,
        visible: Range<usize>,
        out: &mut Vec<(Range<usize>, &'static str)>,
    ) {
        if self.drawn == Some(text.rev()) {
            let shown =
                |(r, _): &&(Range<usize>, &str)| r.start < visible.end && r.end > visible.start;
            out.extend(self.spans.iter().filter(shown).cloned());
        }
    }
}

/// Lines of text, and faces on byte ranges of them.
#[derive(Default)]
struct Page {
    text: String,
    spans: Vec<(Range<usize>, &'static str)>,
}

impl Page {
    fn line(&mut self, line: &str, face: Option<&'static str>) {
        let at = self.text.len();
        self.text.push_str(line);
        if let Some(face) = face.filter(|_| !line.is_empty()) {
            self.spans.push((at..self.text.len(), face));
        }
        self.text.push('\n');
    }
}

impl Browser {
    /// Draw it in `buf`, at the size of the window showing it: seek's layout.
    fn draw(&mut self, ctx: &mut Ctx, buf: BufferId) -> Result {
        let (width, body) = size(ctx, buf);
        let now = SystemTime::now();
        let all = notes(&self.dir);
        let rows: Vec<&Note> = all
            .iter()
            .filter(|n| fuzzy_match(&self.query, &n.name))
            .collect();
        let shown = MAX_VISIBLE.min(body.saturating_sub(CHROME).max(1));
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
        if self.selected >= self.scroll + shown {
            self.scroll = self.selected + 1 - shown;
        }
        self.on = rows.get(self.selected).map(|n| n.name.clone());

        let rule = "─".repeat(width);
        let title_len = self.title.chars().count();
        let left = width.saturating_sub(title_len) / 2;
        let right = width.saturating_sub(left + title_len);
        let mut page = Page::default();
        let title = format!("{}{}{}", "─".repeat(left), self.title, "─".repeat(right));
        page.line(&title, Some("bold"));
        page.line(&rule, Some("shadow"));
        let search = self.searching.then_some(SEARCH_FACE);
        page.line(&padded(&self.query, width), search);
        page.line(&rule, Some("shadow"));
        let columns = format!("{:<NAME_WIDTH$} {}", "NAME", "UPDATED");
        page.line(&columns, Some("bold"));
        page.line(&rule, Some("shadow"));
        if rows.is_empty() {
            page.line("No results", Some("shadow"));
        }
        for (i, note) in rows.iter().enumerate().skip(self.scroll).take(shown) {
            let line = padded(&row(note, now), width);
            page.line(&line, (i == self.selected).then_some("highlight"));
        }

        let windows: Vec<WindowId> = ctx
            .window_list()
            .into_iter()
            .filter(|w| ctx.window_buffer(*w) == Some(buf))
            .collect();
        let t = ctx.text_mut(buf)?;
        if *t.rope() != *page.text {
            let whole = 0..t.len();
            t.tx(Origin::Extension)
                .silent()
                .replace(whole, page.text)
                .commit()?;
            t.set_modified(false);
        }
        self.drawn = Some(t.rev());
        self.spans = page.spans;
        // The cursor isn't drawn: it sits on the selected row, so a short window shows it.
        let at = t.line_start(CHROME + self.selected - self.scroll);
        for w in windows {
            t.set_scroll(w, 0, 0);
            t.set_cursor(w, at);
        }
        Ok(())
    }

    /// Search, or stop.
    fn set_searching(&mut self, ctx: &mut Ctx, buf: BufferId, on: bool) {
        if self.searching != on {
            self.searching = on;
            let _ = if on {
                ctx.enable_mode::<Search>(buf)
            } else {
                ctx.disable_mode::<Search>(buf)
            };
        }
    }
}

/// The width the browser is drawn at and the rows it has: those of the window showing it, the
/// focused one first. Shown nowhere, 80 by 24.
fn size(ctx: &Ctx, buf: BufferId) -> (usize, usize) {
    let showing = |w: &WindowId| ctx.window_buffer(*w) == Some(buf);
    let w = ctx
        .focused()
        .filter(showing)
        .or_else(|| ctx.window_list().into_iter().find(showing));
    let g = w.and_then(|w| ctx.text(buf).ok()?.geometry(w));
    g.map_or((80, 24), |g| (g.text_w, g.rows))
}

/// A checkout's result, with a killed buffer, or one without the mode, as None.
fn quiet<T>(r: std::result::Result<T, Checkout>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(Checkout::Gone | Checkout::Missing) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Draw the browser in `buf` again. Nothing if it's gone.
fn redraw(ctx: &mut Ctx, buf: BufferId) -> Result {
    quiet(ctx.with_mode(buf, |b: &mut Browser, ctx| b.draw(ctx, buf)))?.unwrap_or(Ok(()))
}

/// What the browser in `buf` styles, as each face with the text it covers, in order.
pub fn faces(ctx: &mut Ctx, buf: BufferId) -> Result<Vec<(String, &'static str)>> {
    let spans = ctx.with_mode(buf, |b: &mut Browser, _| b.spans.clone())?;
    let text = ctx.text(buf)?.rope().to_string();
    let cover = |(r, f): (Range<usize>, &'static str)| text.get(r).map(|s| (s.to_string(), f));
    Ok(spans.into_iter().filter_map(cover).collect())
}

// ------------------------------------------------------------------ commands

/// The browser a command acts on: its buffer, or the focused one, which must be the browser.
fn target(ctx: &mut Ctx, args: &Args) -> Result<BufferId> {
    let not_here = || Error::Msg(format!("Not in {BUFFER}"));
    let buf = args
        .buffer
        .or_else(|| ctx.focused_buffer())
        .ok_or_else(not_here)?;
    quiet(ctx.with_mode(buf, |_: &mut Browser, _| ()))?.ok_or_else(not_here)?;
    Ok(buf)
}

/// Change the browser a command acts on with `f`, then draw it.
fn change(ctx: &mut Ctx, args: &Args, f: impl FnOnce(&mut Browser, &mut Ctx, BufferId)) -> Result {
    let buf = target(ctx, args)?;
    ctx.with_mode(buf, |b: &mut Browser, ctx| f(b, ctx, buf))?;
    redraw(ctx, buf)
}

/// `RET`: open the selected note in this window as a file, and kill the browser.
fn open(ctx: &mut Ctx, args: &Args) -> Result {
    let buf = target(ctx, args)?;
    let (dir, on) = ctx.with_mode(buf, |b: &mut Browser, _| (b.dir.clone(), b.on.clone()))?;
    let Some(name) = on else {
        ctx.messages.push("No note selected".into());
        return Ok(());
    };
    let note = ctx.open_file(&dir.join(format!("{name}.{EXTENSION}")))?;
    ctx.show(note, ShowIn::Current)?;
    ctx.kill(buf);
    Ok(())
}

/// `q`: kill the browser. Its window shows the buffer used last.
fn quit(ctx: &mut Ctx, args: &Args) -> Result {
    let buf = target(ctx, args)?;
    ctx.kill(buf);
    Ok(())
}

/// Move the selection by `by` rows.
fn move_by(ctx: &mut Ctx, args: &Args, by: isize) -> Result {
    change(ctx, args, |b, _, _| {
        b.selected = b.selected.saturating_add_signed(by);
    })
}

/// A printable key while searching: add it to the query, and go back to the first row.
fn insert(ctx: &mut Ctx, args: &Args) -> Result {
    let Some(typed) = args.typed() else {
        return Ok(());
    };
    change(ctx, args, |b, _, _| {
        b.query.push_str(&typed);
        (b.selected, b.scroll) = (0, 0);
    })
}

/// `DEL` while searching: take the query's last character back.
fn backspace(ctx: &mut Ctx, args: &Args) -> Result {
    change(ctx, args, |b, _, _| {
        if b.query.pop().is_some() {
            (b.selected, b.scroll) = (0, 0);
        }
    })
}

type Command = fn(&mut Ctx, &Args) -> Result;
/// Keys, in notation.
type Keys = &'static [&'static str];

/// The browser's commands, their docs, and their keys in [`MODE`] and in [`SEARCH_MODE`].
const COMMANDS: &[(&str, &str, Command, Keys, Keys)] = &[
    (
        "scribble-open",
        "Open the selected note and close the browser.",
        open,
        &["<CR>"],
        &[],
    ),
    (
        "scribble-quit",
        "Close the note browser.",
        quit,
        &["q"],
        &[],
    ),
    (
        "scribble-next",
        "Select the next note.",
        |c, a| move_by(c, a, 1),
        &["j", "<Down>", "<C-n>"],
        &[],
    ),
    (
        "scribble-previous",
        "Select the previous note.",
        |c, a| move_by(c, a, -1),
        &["k", "<Up>", "<C-p>"],
        &[],
    ),
    (
        "scribble-search",
        "Type a query to filter the notes by name.",
        |c, a| change(c, a, |b, ctx, buf| b.set_searching(ctx, buf, true)),
        &["/"],
        &[],
    ),
    (
        "scribble-search-exit",
        "Stop typing the query, keeping it.",
        |c, a| change(c, a, |b, ctx, buf| b.set_searching(ctx, buf, false)),
        &[],
        &["<Esc>", "<C-g>"],
    ),
    (
        "scribble-search-insert",
        "Add the typed character to the query.",
        insert,
        &[],
        &["<char>"],
    ),
    (
        "scribble-search-backspace",
        "Take the query's last character back.",
        backspace,
        &[],
        &["<BS>"],
    ),
];

// ------------------------------------------------------------------ the extension

/// The extension: your options.
#[derive(Default)]
pub struct Scribble {
    /// Your options.
    pub options: Options,
}

impl Scribble {
    /// The extension with these options.
    pub fn new(options: Options) -> Scribble {
        Scribble { options }
    }

    /// `scribble`: the browser in this window, new, searching, on the first note.
    fn show(&mut self, ctx: &mut Ctx, _: &Args) -> Result {
        let buf = match ctx.find_buffer(BUFFER) {
            Some(buf) => buf,
            None => ctx.create_text(BUFFER, ""),
        };
        ctx.change_settings(buf, |s| {
            (s.wrap, s.read_only, s.cursor_visible, s.undo_steps) = (Wrap::Off, true, false, 0);
        })?;
        ctx.enable_mode::<Browser>(buf)?;
        let (dir, title) = (self.options.dir(), format!(" {} ", self.options.title));
        ctx.with_mode(buf, |b: &mut Browser, ctx| {
            (b.dir, b.title, b.query) = (dir, title, String::new());
            (b.selected, b.scroll) = (0, 0);
            b.set_searching(ctx, buf, true);
        })?;
        ctx.show(buf, ShowIn::Current)?;
        redraw(ctx, buf)
    }
}

impl Extension for Scribble {
    fn name(&self) -> &'static str {
        "scribble"
    }

    fn register(&mut self, reg: &mut Registry) {
        reg.face(SEARCH_FACE, Some("highlight"), Style::new());
        reg.ext_command(
            "scribble",
            "Browse your notes: type to search them by name, RET to open one.",
            Scribble::show,
        );
        for (name, doc, f, _, _) in COMMANDS {
            reg.command(name, doc, *f);
        }
        let mode = reg.mode(MODE, Browser::default);
        for (name, _, _, keys, _) in COMMANDS {
            for k in *keys {
                mode.bind(State::Normal, k, name);
            }
        }
        let search = reg.mode(SEARCH_MODE, || Search);
        for (name, _, _, _, keys) in COMMANDS {
            for k in *keys {
                search.bind(State::Normal, k, name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_is_fuzzy_and_ignores_case_and_rows_are_cut_at_41() {
        assert!(fuzzy_match("mtg", "Meeting notes"));
        assert!(fuzzy_match("MN", "meeting notes"));
        assert!(!fuzzy_match("nm", "meeting notes"));
        assert!(fuzzy_match("", "anything"));
        let now = SystemTime::now();
        let ago = |s: u64| relative_time(now - Duration::from_secs(s), now);
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1 minutes ago");
        assert_eq!(ago(3599), "59 minutes ago");
        assert_eq!(ago(7200), "2 hours ago");
        assert_eq!(ago(86400 * 3), "3 days ago");
        let long = Note {
            name: "x".repeat(50),
            modified: now,
        };
        assert_eq!(row(&long, now), format!("{}… just now", "x".repeat(41)));
    }
}

//! The browser in a harness with die-vim, on made-up notes in a temporary directory: the layout
//! and the order, the fuzzy filter, the keys in and out of search, opening a note, quitting, and
//! a missing directory.

use die_test_cap as _;

use die_core::{Args, BufferId, Harness};
use die_scribble::{BUFFER, Options, Scribble, faces};
use die_vim::Vim;
use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// A directory of notes that goes when the test ends.
struct Notes(PathBuf);

impl Notes {
    fn new(tag: &str) -> Notes {
        let dir = std::env::temp_dir().join(format!("die-scribble-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Notes(dir)
    }

    /// A file named `file`, written `secs` ago.
    fn file(&self, file: &str, text: &str, secs: u64) -> &Notes {
        let path = self.0.join(file);
        fs::write(&path, text).unwrap();
        let when = SystemTime::now() - Duration::from_secs(secs);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(when)
            .unwrap();
        self
    }
}

impl Drop for Notes {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Four notes, and a hidden one and a text file that aren't notes.
fn four(tag: &str) -> Notes {
    let notes = Notes::new(tag);
    notes
        .file("groceries.md", "# Groceries\n", 5 * 60)
        .file("meeting notes.md", "# Meeting\n", 0)
        .file("rust ideas.md", "# Rust\n", 3 * 3600)
        .file(&format!("{}.md", "a".repeat(45)), "long\n", 2 * 86400)
        .file(".hidden.md", "hidden\n", 0)
        .file("todo.txt", "not a note\n", 0);
    notes
}

/// die-vim and scribble on `dir`, with a buffer `notes` on screen in an 80 x 24 window.
fn harness(dir: PathBuf) -> (Harness, BufferId) {
    let options = Options {
        directory: Some(dir),
        ..Options::default()
    };
    let mut h = Harness::new(vec![
        Box::new(Vim::default()),
        Box::new(Scribble::new(options)),
    ])
    .unwrap();
    let (notes, _) = h.open_text("notes", "notes\n").unwrap();
    (h, notes)
}

/// `scribble`, as `\s` runs it.
fn scribble(h: &mut Harness) -> BufferId {
    h.run("scribble", &Args::default()).unwrap();
    h.run_until_idle();
    h.find_buffer(BUFFER).expect("the browser")
}

fn keys(h: &mut Harness, keys: &str) {
    for k in h.type_keys(keys).unwrap() {
        assert!(k.errors.is_empty(), "{}: {:?}", k.key, k.errors);
    }
}

fn lines(h: &mut Harness) -> Vec<String> {
    h.screen().lines().map(String::from).collect()
}

/// The rows on screen: the lines under the chrome, to the first empty one.
fn rows(h: &mut Harness) -> Vec<String> {
    let lines = lines(h);
    lines[6..]
        .iter()
        .take_while(|l| !l.is_empty())
        .cloned()
        .collect()
}

/// The selected row, without its padding.
fn selected(h: &mut Harness, buf: BufferId) -> String {
    let faces = faces(h, buf).unwrap();
    let row = faces.iter().find(|(_, f)| *f == "highlight");
    row.map_or(String::new(), |(t, _)| t.trim_end().to_string())
}

fn row(name: &str, updated: &str) -> String {
    format!("{name:<42} {updated}")
}

#[test]
fn it_draws_the_title_the_search_line_the_columns_and_the_notes_newest_first() {
    let notes = four("layout");
    let (mut h, _) = harness(notes.0.clone());
    let buf = scribble(&mut h);
    assert_eq!(h.focused_buffer(), Some(buf), "in this window");
    assert_eq!(h.window_list().len(), 1, "never a split");
    let rule = "─".repeat(80);
    let title = format!("{} scribble {}", "─".repeat(35), "─".repeat(35));
    let long = format!("{}…", "a".repeat(41));
    let expected = [
        title.as_str(),
        &rule,
        "",
        &rule,
        &row("NAME", "UPDATED"),
        &rule,
        &row("meeting notes", "just now"),
        &row("groceries", "5 minutes ago"),
        &row("rust ideas", "3 hours ago"),
        &row(&long, "2 days ago"),
        "",
    ];
    let screen = lines(&mut h);
    assert_eq!(screen[..expected.len()], expected, "{}", screen.join("\n"));

    let faces = faces(&mut h, buf).unwrap();
    let faces: Vec<(&str, &str)> = faces.iter().map(|(t, f)| (t.trim_end(), *f)).collect();
    assert_eq!(
        faces,
        [
            (title.as_str(), "bold"),
            (&rule, "shadow"),
            ("", die_scribble::SEARCH_FACE),
            (&rule, "shadow"),
            (&row("NAME", "UPDATED"), "bold"),
            (&rule, "shadow"),
            (&row("meeting notes", "just now"), "highlight"),
        ],
        "searching from the start, on the first note"
    );
    let t = h.text(buf).unwrap();
    let s = t.settings();
    assert!(s.read_only && !s.cursor_visible, "read-only, no cursor");

    // A narrower window draws it again at its width.
    h.set_window_size(60, 24);
    assert_eq!(lines(&mut h)[1], "─".repeat(60));
}

#[test]
fn typing_filters_by_a_fuzzy_name_and_backspace_takes_it_back() {
    let notes = four("filter");
    let (mut h, _) = harness(notes.0.clone());
    let buf = scribble(&mut h);
    keys(&mut h, "RI");
    assert_eq!(lines(&mut h)[2].trim_end(), "RI");
    assert_eq!(
        rows(&mut h),
        [
            row("groceries", "5 minutes ago"),
            row("rust ideas", "3 hours ago")
        ],
        "r..i in order, whatever the case"
    );
    keys(&mut h, "d");
    assert_eq!(rows(&mut h), [row("rust ideas", "3 hours ago")]);
    keys(&mut h, "zq");
    assert_eq!(rows(&mut h), ["No results"]);
    assert_eq!(selected(&mut h, buf), "");
    keys(&mut h, "<BS><BS><BS><BS>");
    assert_eq!(lines(&mut h)[2].trim_end(), "R");
    assert_eq!(rows(&mut h).len(), 2, "r alone: meeting notes has none");
}

#[test]
fn search_and_normal_keys_move_and_esc_and_c_g_keep_the_query() {
    let notes = four("keys");
    let (mut h, _) = harness(notes.0.clone());
    let buf = scribble(&mut h);
    // In search, j and k are the query, and Up, Down, C-n and C-p move.
    keys(&mut h, "<Down><Down><Up><C-n><C-p>");
    assert_eq!(selected(&mut h, buf), row("groceries", "5 minutes ago"));
    keys(&mut h, "e");
    assert_eq!(
        selected(&mut h, buf),
        row("meeting notes", "just now"),
        "back to the top"
    );
    keys(&mut h, "<Esc>");
    assert_eq!(lines(&mut h)[2].trim_end(), "e", "the query kept");
    let search = |h: &mut Harness| {
        let faces = faces(h, buf).unwrap();
        faces.iter().any(|(_, f)| *f == die_scribble::SEARCH_FACE)
    };
    assert!(!search(&mut h), "not searching");
    assert_eq!(
        rows(&mut h).len(),
        3,
        "groceries, meeting notes, rust ideas"
    );

    // Out of search: j, k, Down, Up, C-n and C-p move, and stop at the ends.
    keys(&mut h, "jj");
    assert_eq!(selected(&mut h, buf), row("rust ideas", "3 hours ago"));
    keys(&mut h, "j<Down><C-n>");
    assert_eq!(
        selected(&mut h, buf),
        row("rust ideas", "3 hours ago"),
        "the last"
    );
    keys(&mut h, "k<Up><C-p>k");
    assert_eq!(
        selected(&mut h, buf),
        row("meeting notes", "just now"),
        "the first"
    );
    keys(&mut h, "<C-n>");
    assert_eq!(selected(&mut h, buf), row("groceries", "5 minutes ago"));

    // `/` searches again, from the query; C-g leaves it too.
    keys(&mut h, "/r");
    assert!(search(&mut h));
    assert_eq!(lines(&mut h)[2].trim_end(), "er");
    assert_eq!(rows(&mut h), [row("groceries", "5 minutes ago")]);
    keys(&mut h, "<C-g>");
    assert!(!search(&mut h));
    assert_eq!(lines(&mut h)[2].trim_end(), "er");
    keys(&mut h, "j");
    assert_eq!(selected(&mut h, buf), row("groceries", "5 minutes ago"));
}

#[test]
fn ret_opens_the_selected_note_as_a_file_in_this_window_and_closes_the_browser() {
    let notes = four("open");
    let (mut h, _) = harness(notes.0.clone());
    scribble(&mut h);
    keys(&mut h, "gro<CR>");
    assert_eq!(h.find_buffer(BUFFER), None, "the browser is gone");
    let note = h.focused_buffer().unwrap();
    assert_eq!(h.file(note).unwrap().path, notes.0.join("groceries.md"));
    assert_eq!(h.text(note).unwrap().rope().to_string(), "# Groceries\n");
    let s = h.text(note).unwrap().settings();
    assert!(!s.read_only && s.cursor_visible, "an ordinary file");
    keys(&mut h, "ddu");

    // And out of search, on the row selected.
    scribble(&mut h);
    keys(&mut h, "<Esc>jj<CR>");
    let note = h.focused_buffer().unwrap();
    assert_eq!(h.file(note).unwrap().path, notes.0.join("rust ideas.md"));
    assert_eq!(h.window_list().len(), 1);
}

#[test]
fn q_quits_back_to_the_buffer_before_and_scribble_again_starts_afresh() {
    let notes = four("quit");
    let (mut h, before) = harness(notes.0.clone());
    let buf = scribble(&mut h);
    keys(&mut h, "q");
    assert_eq!(rows(&mut h).len(), 1, "q is the query while searching");
    keys(&mut h, "<Esc>q");
    assert_eq!(h.find_buffer(BUFFER), None);
    assert_eq!(h.focused_buffer(), Some(before));

    let again = scribble(&mut h);
    assert_ne!(again, buf);
    assert_eq!(lines(&mut h)[2], "", "no query");
    assert_eq!(rows(&mut h).len(), 4);
    assert_eq!(selected(&mut h, again), row("meeting notes", "just now"));
}

#[test]
fn a_missing_or_empty_directory_shows_no_notes() {
    let notes = Notes::new("empty");
    let (mut h, _) = harness(notes.0.join("missing"));
    let buf = scribble(&mut h);
    assert_eq!(rows(&mut h), ["No results"]);
    let log = h.type_keys("<CR>").unwrap();
    assert!(log[0].errors.is_empty(), "{:?}", log[0].errors);
    assert_eq!(h.focused_buffer(), Some(buf), "nothing to open");
    assert_eq!(
        h.messages.last().map(String::as_str),
        Some("No note selected")
    );
    keys(&mut h, "<Esc>q");

    let (mut h, _) = harness(notes.0.clone());
    scribble(&mut h);
    assert_eq!(rows(&mut h), ["No results"]);
}

#[test]
fn the_directory_defaults_to_config_scribble_under_home() {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    assert_eq!(Options::default().dir(), home.join(".config/scribble"));
    assert_eq!(Options::default().title, "scribble");
}

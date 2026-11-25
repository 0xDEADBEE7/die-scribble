# Using die-scribble

`:scribble`, or `\s` with the binding from the README, shows the browser in this window, in a
read-only buffer `*scribble*` with no cursor drawn. It opens searching, with an empty query and
the first note selected; running it again starts afresh.

```text
───────────────────────── scribble ─────────────────────────
────────────────────────────────────────────────────────────
<the query>
────────────────────────────────────────────────────────────
NAME                                       UPDATED
────────────────────────────────────────────────────────────
meeting notes                              just now
groceries                                  5 minutes ago
```

The notes are the `.md` files in the directory, hidden ones left out, the one changed last
first. A row is the file's name without `.md`, cut at 41 characters with `…`, and how long ago
it changed: `just now`, `N minutes ago`, `N hours ago` or `N days ago`. The selected row is
highlighted, and a window shows at most 20 rows, scrolling to keep the selection on screen. A
missing or empty directory, or a query nothing matches, shows `No results`.

## Keys

In normal state, the browser's only one.

| Key | While searching | Otherwise |
|---|---|---|
| a character | Add it to the query | |
| `Backspace` | Take the query's last character back | |
| `Up`, `Down` | Move | Move |
| `j`, `k`, `C-n`, `C-p` | Typed into the query (`C-n`, `C-p` move) | Move |
| `RET` | Open the selected note | Open the selected note |
| `Esc`, `C-g` | Stop searching, keeping the query | |
| `/` | Typed into the query | Search again |
| `q` | Typed into the query | Close the browser |

Typing or deleting goes back to the first row. The query matches a note whose name has its
characters in order, whatever the case: `mtg` finds `Meeting notes`.

Opening a note kills the browser and shows `<directory>/<name>.md` in its window as an ordinary
file buffer. Closing the browser shows the buffer you used before it.

## Options

`die_scribble::Options`, passed with `Scribble::new(Options { .. })`:

| Option | Default | What it does |
|---|---|---|
| `directory` | `None`: `$HOME/.config/scribble/`, read when `scribble` runs | Where the notes are |
| `title` | `"scribble"` | The title in the top rule |

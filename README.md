# die-scribble

A browser for your markdown notes in a [DIE](https://github.com/0xDEADBEE7/die) buffer: search
one by name and open it.

- `:scribble` (bound to `\s` by your config) shows `*scribble*` in this window, searching.
- A row per `.md` file in `~/.config/scribble/`, newest first, with how long ago it changed.
- Typing filters by name, fuzzily: the letters in order, whatever the case.
- `RET` opens the selected note as an ordinary file, in place of the browser.
- An extension of its own: it depends on die-core only.

| Key | What it does |
|---|---|
| any character, `Backspace` | Edit the query, while searching |
| `Up` `Down`, and `j` `k` `C-n` `C-p` out of search | Move |
| `RET` | Open the selected note |
| `Esc`, `C-g` | Stop searching, keeping the query |
| `/` | Search again |
| `q` | Close the browser |

## Install

Add it to your config's `Cargo.toml`:

```toml
die-scribble = { git = "https://github.com/0xDEADBEE7/die-scribble", branch = "main" }
```

to its list of extensions:

```rust
Box::new(die_scribble::Scribble::default()),
```

and bind it:

```rust
keys.bind(State::Normal, "<leader>s", "scribble");
```

See [docs/usage.md](docs/usage.md) for the keys and options.

## Licence

MIT or Apache-2.0, at your option: `LICENSE-MIT`, `LICENSE-APACHE`.

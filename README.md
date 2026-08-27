# brain

A local-first "second brain": a personal knowledge base you drive from the terminal. Capture fleeting notes, journal, jot dev notes, bookmark references, and pull coding-session context back out with **Nexus-style curated retrieval** — every answer anchored to a `file:line` citation.

Everything is **plain-text Markdown** in a directory you own. No cloud, no lock-in. Write with any editor, search instantly, leave whenever you want.

## Why

Your existing knowledge tools are fragmented — quick captures scattered, dev notes buried in repos, bookmarks lost, and coding-session context gone the moment the terminal closes. `brain` unifies them behind one calm, keyboard-first interface with a buildable index and provenance-cited retrieval, mirroring the Nexus architecture: a raw full-text index plus a curated retrieval layer.

## Install

```bash
cargo build --release
cp target/release/brain ~/.local/bin/brain
cp target/release/studio ~/.local/bin/studio
```

Both binaries live under `~/.local/bin` (on PATH). `brain` is the CLI; `studio` is the interactive TUI.

## Quick start

```bash
# capture a throwaway note
brain snap "remember to water the rosemary plant" -t home,plant

# journal (appends to today's journal file with a timestamp)
brain snap --kind journal "what I did today"

# dev/technical note
brain snap --kind dev "learn rust ownership and borrowing" -t rust -T "rust notes"

# bookmark a link
brain snap --kind link -T "rust book" --url https://doc.rust-lang.org/book

# add coding-session context (indexed automatically)
brain session "chat log: decided to use Cloudflare Workers for deployment"

# rebuild the keyword index
brain index

# keyword full-text search
brain find rust

# Nexus-style curated retrieval with file:line provenance
brain ask "how did we decide the deployment?"

# today's view
brain daily

# open the TUI
brain studio
```

## Commands

| Command                | Description                                             |
| ---------------------- | ------------------------------------------------------- |
| `brain snap [TEXT]`    | Capture a note. `--kind capture\|journal\|dev\|link\|session`, `-T` title, `-t` tags, `--url` for links |
| `brain edit <path>`    | Open a note in `$EDITOR`                               |
| `brain session <txt>`  | Append coding-session/context text to a session note    |
| `brain find <query>`   | Keyword full-text search with snippets + highlighting    |
| `brain ask <query>`    | Curated retrieval, cited to `file:line`                 |
| `brain index`          | Rebuild the keyword index                                |
| `brain daily`          | Today's notes + recent captures                          |
| `brain studio`         | Launch the interactive TUI                               |

## Config

Config lives at `~/.config/brain/config.toml` (or pass `--config <path>`).

```toml
# required-ish: where your notes live (defaults to ~/brain)
notes_dir = "~/brain"

# where the index is cached (defaults to <notes_dir>/.brain/index)
# index_dir = "~/.cache/brain/index"

# editor used by `brain edit` and the TUI `e` key (defaults to $EDITOR)
# editor = "nvim"

# ignore these subdirectories when walking the corpus
# ignore = [".git", ".brain"]
```

## Storage layout

```
~/brain/
├── capture/            # fleeting notes (`brain snap`)
├── dev/                # technical/dev notes (`brain snap --kind dev`)
├── link/               # bookmarks (`brain snap --kind link`)
├── session/            # coding-session context (`brain session`)
├── journal-YYYY-MM-DD.md   # daily journal (`brain snap --kind journal`)
└── .brain/index/       # buildable index cache (regenerable)
```

Every note is a `.md` file with YAML-style front matter (`kind`, `created`, `tags`, `title`) — readable and editable by hand at any time.

## TUI (studio)

`studio` is a keyboard-navigable note browser:

- `j`/`k` or arrows — move through notes
- `g`/`G` / `G` — jump to top / bottom
- `/` — search (Enter to run, Esc to cancel)
- `a` — Nexus-style `ask` retrieval
- `n` — new quick capture
- `e` — open the selected note in your editor
- `r` — re-index the corpus
- `q` — quit

## How retrieval works

- **`find`** — an on-disk inverted index with term frequency + filename boosts. Instant substring/token search over the whole corpus.
- **`ask`** — curates the corpus into retrievable chunks (split on headings/blank lines), ranks them against your query with TF-IDF-style scoring, and returns the top chunks anchored to exact `file:line` citations. Deterministic and fully offline — no LLM required.

The index is a disposable cache: `brain index` rebuilds it from raw Markdown at any time.

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Layout (mirrors the `homectl` convention):

```
brain/
├── brain-core/   # storage, indexing, chunking, retrieval
├── brain-cli/    # the `brain` CLI binary
└── brain-tui/    # the `studio` TUI binary
```

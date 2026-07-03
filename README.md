# kachet

Keyboard-first double-entry accounting for the web. A GnuCash replacement (not a clone):
Rust + SQLite backend, Svelte frontend, built for fast data entry and navigation.

## Quick start

Requires Rust, Node.js, and [just](https://github.com/casey/just):

```sh
just serve                        # from a fresh clone: installs, builds, runs
                                  # → http://127.0.0.1:8710
just import your-file.gnucash     # optional: import GnuCash XML (gzipped or plain)
just dev                          # development: hot-reloading frontend on :5173
```

Without just, the equivalent is:

```sh
(cd web && npm install && npm run build)            # must run before cargo build
cargo build --release                               # embeds web/dist into the binary
./target/release/kachet import your-file.gnucash
./target/release/kachet serve
```

The result is a single self-contained binary — frontend assets are embedded at
compile time. During development, `kachet serve --static-dir web/dist` serves
from disk instead (or use `npm run dev` for hot reload, which proxies /api).

The header shows a live backend connection indicator (websocket with
auto-reconnect). When the server comes back with newer frontend assets than
the loaded page (mtime-hash comparison — "webhash"), a banner offers a reload
at your convenience rather than reloading out from under you.

## Keyboard reference

### Accounts view
| Key | Action |
|---|---|
| ↑ ↓ PgUp PgDn Home End | move selection |
| ← → | collapse / expand subtree (← also jumps to parent) |
| Enter | open account register |
| any text | filter accounts (Esc clears) |
| Insert | new account (defaults to nesting under a selected placeholder, else as sibling) |
| F2 | edit selected account |
| Delete | delete selected account (only when it has no transactions or children) |

### Register view
| Key | Action |
|---|---|
| ↑ ↓ PgUp PgDn Home End | move selection (moves through split rows when expanded) |
| `[` / `]` | jump back / forward one month |
| `{` / `}` | jump back / forward one year |
| Space | open/close splits for selected transaction |
| Enter or `e` | edit selected transaction |
| `n` | new transaction |
| `j` | jump to transfer account (or the account of a selected split row) |
| `d` / Delete | delete selected transaction |
| Esc / Backspace | back to previous register / accounts |

### Transaction editor
| Key | Action |
|---|---|
| Tab / Shift+Tab | move between fields |
| Insert or Alt+S | add a split line |
| Enter | save (leaving exactly one amount blank auto-balances it) |
| Esc | cancel |

## Status

Early working version: import, account tree, register navigation, transaction
entry/edit/delete with multi-split support and zero-sum validation. Reports,
multi-currency entry, fiscal-year-aware reporting, and Postgres support are planned.

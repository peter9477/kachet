# kachet

Keyboard-first double-entry accounting for the web. A GnuCash replacement (not a clone):
Rust + SQLite backend, Svelte frontend, built for fast data entry and navigation.

## Quick start

```sh
cargo build --release
(cd web && npm install && npm run build)
./target/release/kachet import your-file.gnucash    # GnuCash XML, gzipped or plain
./target/release/kachet serve                       # http://127.0.0.1:8710
```

## Keyboard reference

### Accounts view
| Key | Action |
|---|---|
| ↑ ↓ PgUp PgDn Home End | move selection |
| ← → | collapse / expand subtree (← also jumps to parent) |
| Enter | open account register |
| any text | filter accounts (Esc clears) |

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

# kachet

Keyboard-first accounting app (GnuCash replacement, not a clone). Rust backend + no-build Vue 3 web UI.
Reports must support non-standard fiscal years/quarters (e.g. a fiscal year ending July 31).

PRIVACY: never commit the user's bookkeeping data or details derived from it (file names,
account names, balances, counts, company/customer names). Verification data points live in
session memory (`kachet-project-goals`), not in this repo.

## Architecture

- `src/` — Rust binary: axum HTTP server + GnuCash XML importer.
  - `db.rs` — SQLite schema via sqlx (kept Postgres-compatible: no SQLite-only types).
    Includes `settings` (key + JSON value; e.g. `fiscal_year_end_month`, default 7).
  - `money.rs` — exact rational (num/denom) amount arithmetic; never floats for stored amounts.
  - `import.rs` — GnuCash XML (gzipped or plain) importer.
  - `api.rs` — REST API + static file serving.
  - `report.rs` — balance sheet / income statement computation (generic row-table JSON)
    + typst markup generation; `pdf.rs` — embedded typst compiler for PDF export.
- `web/` — plain-JavaScript Vue 3 frontend, NO build step (user's explicit preference:
  no Node/npm toolchain). Vue is vendored at `web/vendor/vue.esm-browser.prod.js`
  (full build incl. runtime template compiler), wired via an importmap in index.html.
  Components are .js files with template strings; JSDoc for types.
- Amounts are stored as integer `num/denom` pairs (GnuCash-style) in both
  transaction currency (`value`) and account commodity (`quantity`).
  Splits must sum to zero in transaction currency; API enforces this.

## Build & run

```sh
cargo build                        # embeds web/ via rust-embed; no frontend build exists
./target/debug/kachet --db kachet.db import <file>.gnucash   # idempotent (INSERT OR REPLACE)
./target/debug/kachet --db kachet.db serve                        # http://127.0.0.1:8710
# dev: `just dev` serves web/ from disk — edit JS, reload browser
```

Note: cargo target dir may be redirected (check `echo $CARGO_TARGET_DIR`; it was /var/tmp/target).

## Verification

Verified against the user's real multi-decade GnuCash book: import counts match the XML
exactly, every transaction's splits sum to zero, and spot-checked account balances match
GnuCash to the cent. Specific figures: see session memory, not this file.

## Conventions

- Keyboard-first: every UI feature must be reachable without the mouse; mouse is backup.
- Commands bind to Ctrl-chords, never bare letters (stray-keypress safety); rationale
  and the reserved-shortcut list live in `doc/decisions.md` — log new decisions there.
- Mouse parity: keyboard-first, but no feature may be keyboard-only — every action
  also needs a mouse path.
- IDs are 32-char hex GUIDs (GnuCash style) so re-imports stay stable.
- Don't copy GnuCash schema/UI wholesale — improve where it's awkward.
- Priorities and scope: see memory `kachet-project-goals`.

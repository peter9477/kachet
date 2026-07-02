# kachet

Keyboard-first accounting app (GnuCash replacement, not a clone). Rust backend + Svelte web UI.
Reports must support non-standard fiscal years/quarters (e.g. a fiscal year ending July 31).

PRIVACY: never commit the user's bookkeeping data or details derived from it (file names,
account names, balances, counts, company/customer names). Verification data points live in
session memory (`kachet-project-goals`), not in this repo.

## Architecture

- `src/` — Rust binary: axum HTTP server + GnuCash XML importer.
  - `db.rs` — SQLite schema via sqlx (kept Postgres-compatible: no SQLite-only types).
  - `money.rs` — exact rational (num/denom) amount arithmetic; never floats for stored amounts.
  - `import.rs` — GnuCash XML (gzipped or plain) importer.
  - `api.rs` — REST API + static file serving.
- `web/` — Vite + Svelte 5 (runes) + TypeScript frontend.
- Amounts are stored as integer `num/denom` pairs (GnuCash-style) in both
  transaction currency (`value`) and account commodity (`quantity`).
  Splits must sum to zero in transaction currency; API enforces this.

## Build & run

```sh
(cd web && npm run build)          # BEFORE cargo build: web/dist is embedded via rust-embed
cargo build
./target/debug/kachet --db kachet.db import <file>.gnucash   # idempotent (INSERT OR REPLACE)
./target/debug/kachet --db kachet.db serve                        # http://127.0.0.1:8710
# dev: `cargo run -- serve` + `cd web && npm run dev` (vite proxies /api to :8710)
```

Note: cargo target dir may be redirected (check `echo $CARGO_TARGET_DIR`; it was /var/tmp/target).

## Verification

Verified against the user's real multi-decade GnuCash book: import counts match the XML
exactly, every transaction's splits sum to zero, and spot-checked account balances match
GnuCash to the cent. Specific figures: see session memory, not this file.

## Conventions

- Keyboard-first: every UI feature must be reachable without the mouse; mouse is backup.
- IDs are 32-char hex GUIDs (GnuCash style) so re-imports stay stable.
- Don't copy GnuCash schema/UI wholesale — improve where it's awkward.
- Priorities and scope: see memory `kachet-project-goals`.

# kachet build/run recipes. From a fresh clone, `just serve` does everything.
# The frontend is plain JS + vendored Vue served from web/ — no Node required.

default:
    @just --list

# Build the binary (web/ assets are embedded at compile time)
build:
    cargo build --release

# Build and run the server (http://127.0.0.1:8710)
serve db="kachet.db": build
    cargo run --release -- --db {{db}} serve

# Import a GnuCash XML file (gzipped or plain)
import file db="kachet.db": build
    cargo run --release -- --db {{db}} import {{file}}

# Development: serve frontend from disk so edits only need a browser reload
dev db="kachet.db":
    cargo run -- --db {{db}} serve --static-dir web

# Run backend tests
test:
    cargo test

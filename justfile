# kachet build/run recipes. From a fresh clone, `just serve` does everything.

default:
    @just --list

# Install frontend dependencies
setup:
    cd web && npm install

# Build frontend then backend (order matters: web/dist is embedded)
build: setup
    cd web && npm run build
    cargo build --release

# Build everything and run the server (http://127.0.0.1:8710)
serve db="kachet.db": build
    cargo run --release -- --db {{db}} serve

# Import a GnuCash XML file (gzipped or plain)
import file db="kachet.db": build
    cargo run --release -- --db {{db}} import {{file}}

# Development: backend + vite with hot reload (http://localhost:5173)
dev: setup
    #!/usr/bin/env bash
    trap 'kill 0' EXIT
    (cd web && npm run build)   # rust-embed needs web/dist to exist to compile
    cargo run -- serve &
    cd web && npm run dev

# Run backend tests
test:
    cargo test

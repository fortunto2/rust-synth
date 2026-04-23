.PHONY: help dev run test lint format build release clean integration

help:
	@echo "rust-synth — terminal ambient synth"
	@echo ""
	@echo "  make dev          run TUI (debug)"
	@echo "  make run          run TUI (release, low-latency)"
	@echo "  make render       offline WAV render via CLI"
	@echo "  make test         cargo test"
	@echo "  make lint         cargo clippy -D warnings"
	@echo "  make format       cargo fmt"
	@echo "  make check        fmt + clippy + test"
	@echo "  make build        debug build"
	@echo "  make release      optimized build"
	@echo "  make integration  headless render smoke test"
	@echo "  make clean        cargo clean"

dev:
	RUST_LOG=info cargo run --bin rust-synth

run:
	cargo run --release --bin rust-synth

render:
	cargo run --release --bin rust-synth-render -- --duration 30 --out out/render.wav

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

format:
	cargo fmt

check: format lint test

build:
	cargo build

release:
	cargo build --release

integration:
	@mkdir -p out
	cargo run --release --bin rust-synth-render -- --duration 5 --out out/integration.wav
	@test -s out/integration.wav && echo "integration OK: out/integration.wav"

clean:
	cargo clean
	rm -rf out

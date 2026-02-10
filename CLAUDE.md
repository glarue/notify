# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Single-file Rust CLI tool (`notify.rs`) that wraps shell command execution and sends email notifications on completion via SMTP. Includes runtime, exit status, hostname, and optionally captured output. Rust rewrite of a Python utility.

## Build & Run

```bash
cargo build                  # debug build
cargo build --release        # release build
cargo run -- -o -- ls -la    # run with args
cargo install --path .       # install locally
```

No tests exist yet. No linter/formatter configured beyond standard `cargo fmt` / `cargo clippy`.

GitHub Actions builds cross-platform binaries (macOS Intel/ARM, Linux gnu/musl, Windows) on version tags (`v*`). Linux musl builds are recommended for maximum compatibility (statically linked, no glibc dependency).

## Architecture

Single binary, single source file (`notify.rs`). Key components:
- **CLI parsing**: `clap` derive API (`Args` struct)
- **Config**: TOML-based (`Config`, `ServerConfig`, `User` structs) with legacy format fallback, stored via XDG dirs (`directories` crate)
- **Command execution**: `run_shell_command()` — spawns user's `$SHELL -lc`, optionally captures stdout/stderr with 500KB limit via threaded readers
- **Email**: `send_email_tls()` — uses `lettre` with rustls-tls, implicit TLS (port 465) or STARTTLS (port 587), 3 retries with 10s backoff
- **Hostname**: `get_descriptive_hostname()` — uses `whoami::hostname()`, falls back to macOS LocalHostName for better specificity
- **Password**: resolved via `password_env` (env var name) or `password` (plaintext) in config

Config location: `~/Library/Application Support/notify/config.toml` (macOS), `~/.config/notify/config.toml` (Linux). Migrates from legacy `~/.notify.config` on first run.

The binary exits with the wrapped command's exit code.

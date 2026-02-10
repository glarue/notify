# notify

Run shell commands and email completion status with runtime and output.

A Rust rewrite of a Python utility that wraps command execution and sends email notifications on completion, including runtime, exit status, and optionally the command output.

## Features

- Email notifications on command completion (success or failure)
- Includes command runtime, hostname, and exit status
- Optional output capture in email body (size-limited to 500KB)
- Interactive configuration for SMTP server and email addresses
- Secure password handling via environment variables
- TOML-based configuration with XDG standard directory support
- Multiple user email addresses with interactive selection

## Installation

### Quick Install (Recommended)

Run the install script (auto-detects your platform):

```bash
curl -fsSL https://raw.githubusercontent.com/glarue/notify/main/install.sh | bash
```

Or download and inspect first:
```bash
curl -fsSL https://raw.githubusercontent.com/glarue/notify/main/install.sh -o install.sh
bash install.sh
```

### Using Cargo

If you have Rust installed:

```bash
cargo install --git https://github.com/glarue/notify
```

### Manual Installation from GitHub Releases

Download the latest binary for your platform from the [releases page](https://github.com/glarue/notify/releases):

**macOS (Intel):**
```bash
curl -LO https://github.com/glarue/notify/releases/latest/download/notify-macos-x86_64.tar.gz
tar xzf notify-macos-x86_64.tar.gz
sudo mv notify /usr/local/bin/
```

**macOS (Apple Silicon):**
```bash
curl -LO https://github.com/glarue/notify/releases/latest/download/notify-macos-aarch64.tar.gz
tar xzf notify-macos-aarch64.tar.gz
sudo mv notify /usr/local/bin/
```

**Linux:**
```bash
curl -LO https://github.com/glarue/notify/releases/latest/download/notify-linux-x86_64.tar.gz
tar xzf notify-linux-x86_64.tar.gz
sudo mv notify /usr/local/bin/
```

**Windows:**
Download `notify-windows-x86_64.zip` from releases and extract to a directory in your PATH.

### Build from Source

Requires Rust 1.70+:
```bash
git clone https://github.com/glarue/notify
cd notify
cargo install --path .
```

## Configuration

### First-time setup

Run the interactive server configuration:
```bash
notify --setup-server
```

This will prompt you for:
- SMTP server address (e.g., `smtp.gmail.com`)
- SMTP port (465 for SSL, 587 for STARTTLS)
- From email address
- Password (recommended: use environment variable)

Configuration is stored at:
- **macOS**: `~/Library/Application Support/notify/config.toml`
- **Linux**: `~/.config/notify/config.toml`
- **Windows**: `%APPDATA%\notify\config.toml`

### Example config

```toml
[server]
server = "smtp.gmail.com"
port = 587
from_address = "your-email@gmail.com"
password_env = "NOTIFY_PASSWORD"

[[users]]
name = "Me"
email = "my-email@example.com"

[[users]]
name = "Team"
email = "team@example.com"
```

### Set password via environment variable (recommended)

```bash
export NOTIFY_PASSWORD='your-app-password'
```

Add this to your `.zshrc` or `.bashrc` to persist across sessions.

### Add email addresses

```bash
notify --add-email
```

## Usage

### Basic usage

Wrap any command with `notify` (two syntaxes supported):

**Using `--` separator (recommended - supports tab-completion):**
```bash
notify -o -- your-command arg1 arg2
notify -e user@example.com -- long-running-task
```

**Using quoted string (legacy Python-style):**
```bash
notify "your-command arg1 arg2" -o
notify "long-running-task" -e user@example.com
```

### Specify recipient email

```bash
# With --
notify -e user@example.com -- pytest

# With quotes
notify "pytest" -e user@example.com
```

### Include command output in email

```bash
# With --
notify -o -- cargo build --release

# With quotes
notify "cargo build --release" -o
```

### Add identifier to subject line

```bash
notify --ID "Nightly Build" -o -- ./run-tests.sh
# or
notify "./run-tests.sh" --ID "Nightly Build" -o
```

### View current config

```bash
notify --view-config
```

### Dry run (print command without executing)

```bash
notify -d -- echo "test"
# or
notify "echo test" -d
```

## Common Use Cases

**Long-running builds:**
```bash
notify --ID "Production Build" -o -- cargo build --release
```

**Scheduled tasks:**
```bash
0 2 * * * /usr/local/bin/notify -e admin@example.com -- /path/to/backup.sh
```

**Test suites:**
```bash
notify -o -- python -m pytest tests/
```

**Data processing:**
```bash
notify --ID "ETL Pipeline" -e team@example.com -- ./process-data.sh
```

## Gmail Setup

For Gmail, you'll need an [App Password](https://support.google.com/accounts/answer/185833):
1. Enable 2-factor authentication on your Google account
2. Generate an app password at https://myaccount.google.com/apppasswords
3. Use the app password (not your regular password) in the config

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

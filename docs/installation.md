# Installation

## Prerequisites

### Required
- **Rust toolchain** (1.70 or newer)
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Optional (for AI features)
- **LM Studio** - Local LLM server for photo descriptions
  - Download from [lmstudio.ai](https://lmstudio.ai/)
- **Ollama** - Alternative local LLM runtime
  - Install from [ollama.ai](https://ollama.ai/)

### Optional (for better image previews)
- A terminal with graphics protocol support:
  - Kitty, iTerm2, WezTerm (best quality)
  - Konsole, foot, xterm with sixel (good quality)

## Building from Source

### Standard Build

```bash
# Clone the repository
git clone <repository-url>
cd clepho

# Build release version
cargo build --release

# Binary location
./target/release/clepho
```

### Using Nix

```bash
# Enter development shell
nix develop

# Build
cargo build --release
```

### Debug Build

For development with faster compilation:

```bash
cargo build
./target/debug/clepho
```

## First Run

On first launch, Clepho will:

1. Create configuration directory at `~/.config/clepho/`
2. Create default configuration file `config.toml`
3. Create data directory at `~/.local/share/clepho/`
4. Initialize SQLite database `clepho.db`
5. Create thumbnail cache at `~/.cache/clepho/thumbnails/`
6. Create trash directory at `~/.local/share/clepho/.trash/`

## Verifying Installation

```bash
# Run Clepho
./target/release/clepho

# You should see the three-pane file browser
# Press ? for help, q to quit
```

## Updating

```bash
# Pull latest changes
git pull

# Rebuild
cargo build --release
```

The database schema is automatically migrated on startup if needed.

## Troubleshooting

### Build Errors

**Missing OpenSSL:**
```bash
# Debian/Ubuntu
sudo apt install libssl-dev pkg-config

# Fedora
sudo dnf install openssl-devel

# macOS
brew install openssl
```

**Missing SQLite:**
```bash
# Debian/Ubuntu
sudo apt install libsqlite3-dev

# Fedora
sudo dnf install sqlite-devel

# macOS (usually pre-installed)
brew install sqlite
```

### Runtime Issues

**No image previews:**
- Check terminal compatibility (see [Navigation](navigation.md))
- Set `protocol = "halfblocks"` in config for basic support
- Set `protocol = "none"` to disable previews

**Database locked:**
- Ensure only one instance of Clepho is running
- Check for stale lock files in `~/.local/share/clepho/`

**Permission denied:**
- Ensure read access to photo directories
- Ensure write access to config/data directories

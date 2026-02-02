# Clepho

A terminal-based photo manager with AI-powered features. Browse, organize, and catalog your photo collection from the command line.

![License](https://img.shields.io/badge/license-GPL--3.0-blue)

## Features

- **File Browser** - Three-pane vim-style navigation with image previews
- **Photo Scanning** - EXIF extraction, thumbnails, and metadata indexing
- **Duplicate Detection** - Find exact and visually similar duplicates
- **AI Descriptions** - Generate descriptions via local or cloud LLMs
- **Face Recognition** - Detect, cluster, and name people in photos
- **Semantic Search** - Find photos by natural language descriptions
- **File Operations** - Move, rename, rotate, and trash with undo
- **Gallery & Slideshow** - Visual browsing modes

## Quick Start

```bash
# Build
cargo build --release

# Run
./target/release/clepho

# Or with Nix
nix develop && cargo build --release
```

Navigate with `h/j/k/l`, press `s` to scan, `?` for help.

## Keybindings

Clepho uses [Yazi](https://yazi-rs.github.io/)-compatible keybindings:

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `h/j/k/l` | Navigate | `y/x` | Cut |
| `d` | Trash | `p` | Paste |
| `r` | Rename | `.` | Toggle hidden |
| `s` | Scan | `u` | Duplicates |
| `i` | AI describe | `/` | Search |
| `?` | Help | `q` | Quit |

See [docs/keyboard-shortcuts.md](docs/keyboard-shortcuts.md) for complete reference.

## Configuration

Config: `~/.config/clepho/config.toml`

```toml
[llm]
provider = "lmstudio"  # lmstudio, ollama, openai, anthropic
endpoint = "http://127.0.0.1:1234/v1"

[preview]
protocol = "auto"  # auto, sixel, kitty, iterm2, halfblocks
```

See [docs/configuration.md](docs/configuration.md) for all options.

## Documentation

| Topic | Description |
|-------|-------------|
| [Installation](docs/installation.md) | Build requirements and setup |
| [Navigation](docs/navigation.md) | File browser and preview pane |
| [Scanning](docs/scanning.md) | Photo indexing and metadata |
| [Duplicates](docs/duplicates.md) | Finding and managing duplicates |
| [AI Features](docs/ai-features.md) | LLM descriptions and CLIP search |
| [Faces](docs/faces.md) | Face detection and people |
| [File Operations](docs/file-operations.md) | Move, rename, and organize |
| [Keyboard Shortcuts](docs/keyboard-shortcuts.md) | Complete keybinding reference |

## License

GPL-3.0

# Clepho

A terminal user interface (TUI) for managing photo collections. Scan, catalog, deduplicate, and AI-describe your photos from the command line.

## Features

- **Three-column file browser** with vim-style navigation and mouse support
- **Photo scanning** with parallel processing and EXIF metadata extraction
- **Duplicate detection** via SHA256 and perceptual hashing
- **AI descriptions** using local LLM (LM Studio, Ollama) or cloud (OpenAI, Anthropic)
- **Face detection & clustering** with person naming
- **CLIP embeddings** for semantic image search
- **View filtering** - hide dotfiles and non-image files by default
- **Gallery & slideshow** modes for browsing
- **File operations** - move, rename, rotate, trash with restore
- **Scheduled tasks** for batch processing
- **Export** to CSV or JSON

## Installation

```bash
# With Nix
nix develop && cargo build --release

# With Cargo
cargo build --release
```

Binary: `target/release/clepho`

## Usage

```bash
clepho
```

Press `?` for help with all keyboard shortcuts.

### Key Bindings (Yazi-compatible)

| Key | Action |
|-----|--------|
| `h/j/k/l` | Navigate (vim-style) |
| `y/x` | Cut files |
| `p` | Paste files |
| `d` | Move to trash |
| `r` | Rename |
| `.` | Toggle hidden files |
| `s` | Scan directory |
| `u` | Find duplicates |
| `i` | AI describe photo |
| `/` | Semantic search |
| `o` | Open in system viewer |
| `?` | Help |
| `q` | Quit |

## Configuration

Config file: `~/.config/clepho/config.toml`

```toml
[llm]
provider = "lmstudio"  # lmstudio, ollama, openai, anthropic
endpoint = "http://127.0.0.1:1234/v1"
model = "gemma-3-4b"

[preview]
protocol = "auto"  # auto, sixel, kitty, iterm2, halfblocks, none

[trash]
max_age_days = 30
```

## License

GPL-3.0

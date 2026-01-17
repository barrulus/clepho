# Clepho

A terminal user interface (TUI) application for managing photo collections. Scan, catalog, deduplicate, and AI-describe your photos - all from the command line.

## Features

- **Three-column file browser** - yazi-inspired navigation with parent, current, and preview panes
- **Photo scanning** - Recursively scan directories for photos with EXIF metadata extraction
- **Duplicate detection** - Find exact duplicates (SHA256) and similar photos (perceptual hashing)
- **AI descriptions** - Generate photo descriptions using local LLM (LM Studio)
- **Batch processing** - Process entire directories with AI in the background
- **SQLite database** - Persistent storage for metadata and descriptions
- **Vim-style navigation** - h/j/k/l keys, gg/G, and mouse support

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- [LM Studio](https://lmstudio.ai/) (optional, for AI features)

### Using Nix

```bash
nix develop
cargo build --release
```

### Using Cargo

```bash
cargo build --release
```

The binary will be at `target/release/clepho`.

## Usage

```bash
clepho
```

### Keyboard Shortcuts

#### Navigation
| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `h` / `←` / `Backspace` | Go to parent directory |
| `l` / `→` / `Enter` | Enter directory |
| `gg` | Go to top |
| `G` | Go to bottom |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |
| `~` | Go to home directory |

#### Actions
| Key | Action |
|-----|--------|
| `s` | Scan current directory for photos |
| `d` | Find duplicate photos |
| `D` | Describe selected image with AI |
| `P` | Batch process all photos with AI |
| `?` | Show help |
| `q` | Quit |

#### Duplicates View
| Key | Action |
|-----|--------|
| `j/k` | Navigate photos in group |
| `J/K` | Navigate between groups |
| `Space` | Toggle mark for deletion |
| `a` | Auto-select duplicates for deletion |
| `x` | Delete marked photos |
| `Esc` | Exit duplicates view |

## Configuration

Configuration is stored at `~/.config/clepho/config.toml`:

```toml
db_path = "~/.local/share/clepho/clepho.db"

[llm]
endpoint = "http://127.0.0.1:1234/v1"
model = "gemma-3-4b"

[scanner]
image_extensions = ["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "raw", "cr2", "nef", "arw"]
similarity_threshold = 10
```

### LLM Setup

1. Install [LM Studio](https://lmstudio.ai/)
2. Download a vision-capable model (e.g., LLaVA, Gemma with vision)
3. Start the local server (default: http://127.0.0.1:1234)
4. Update `config.toml` with your model name

## Supported Formats

| Format | Extensions |
|--------|------------|
| JPEG | .jpg, .jpeg |
| PNG | .png |
| GIF | .gif |
| WebP | .webp |
| HEIC/HEIF | .heic, .heif |
| RAW | .raw, .cr2, .nef, .arw |

## Database

Clepho stores photo metadata in SQLite at `~/.local/share/clepho/clepho.db`. The database includes:

- File paths and sizes
- EXIF metadata (camera, lens, settings, GPS)
- Hash values for duplicate detection
- AI-generated descriptions and tags

## License

GNU GENERAL PUBLIC LICENSE
Version 3, 29 June 2007

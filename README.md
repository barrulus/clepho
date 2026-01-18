# Clepho

A terminal user interface (TUI) application for managing photo collections. Scan, catalog, deduplicate, and AI-describe your photos - all from the command line.

## Features

- **Three-column file browser** - yazi-inspired navigation with parent, current, and preview panes
- **Photo scanning** - Recursively scan directories with parallel processing for fast indexing
- **EXIF metadata extraction** - Complete EXIF data captured as JSON for all fields
- **Duplicate detection** - Find exact duplicates (SHA256) and similar photos (perceptual hashing)
- **Safe trash system** - Move duplicates to trash with restore capability before permanent deletion
- **AI descriptions** - Generate photo descriptions using local LLM (LM Studio, Ollama, OpenAI)
- **Batch processing** - Process entire directories with AI in the background
- **Face detection** - Detect and cluster faces, assign names to people
- **Semantic search** - Search photos by description content
- **Change detection** - Detect new/modified files when entering directories, with rescan prompts
- **Scheduled tasks** - Schedule scans, LLM batch processing, or face detection for later execution
- **Export** - Export photo metadata to CSV or JSON
- **File operations** - Move, rename (with patterns), and organize photos
- **Thumbnail caching** - Pre-generated thumbnails for instant preview loading
- **SQLite database** - Persistent storage for metadata, descriptions, and face data
- **Vim-style navigation** - h/j/k/l keys, gg/G, and full mouse support

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- [LM Studio](https://lmstudio.ai/) or [Ollama](https://ollama.ai/) (optional, for AI features)

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
| `l` / `→` / `Enter` | Enter directory / select |
| `gg` | Go to top |
| `G` | Go to bottom |
| `Ctrl+f` | Page down |
| `Ctrl+b` | Page up |
| `~` | Go to home directory |
| `{` / `}` | Scroll preview text up/down |

#### File Operations
| Key | Action |
|-----|--------|
| `Space` | Toggle file selection |
| `m` | Move selected files |
| `r` | Rename selected files |
| `e` | Export photo metadata |

#### Scanning & Analysis
| Key | Action |
|-----|--------|
| `s` | Scan current directory for photos |
| `d` | Find duplicate photos |
| `D` | Describe selected image with AI |
| `P` | Batch process all photos with AI |
| `F` | Detect faces in scanned photos |
| `/` | Semantic search |

#### Views & Dialogs
| Key | Action |
|-----|--------|
| `p` | View people (face clusters) |
| `t` | View/manage trash |
| `c` | Check for file changes in current directory |
| `@` | Schedule a task for later execution |
| `?` | Show help |
| `q` | Quit |

#### Duplicates View
| Key | Action |
|-----|--------|
| `j/k` | Navigate photos in group |
| `J/K` | Navigate between groups |
| `Space` | Toggle mark for deletion |
| `a` | Auto-select duplicates (keep highest quality) |
| `x` | Move marked to trash |
| `X` | Permanently delete marked |
| `Esc` | Exit duplicates view |

#### Trash View
| Key | Action |
|-----|--------|
| `j/k` | Navigate trash items |
| `Enter` / `r` | Restore selected file |
| `d` | Permanently delete selected |
| `c` | Cleanup (apply age/size rules) |
| `Esc` | Close trash view |

#### Changes View
| Key | Action |
|-----|--------|
| `Tab` | Switch between New/Modified tabs |
| `j/k` | Navigate file list |
| `Space` | Toggle file selection |
| `a` | Select all files |
| `Enter` | Rescan selected files |
| `q` / `Esc` | Close changes view |

#### Schedule Dialog
| Key | Action |
|-----|--------|
| `Tab` / `j/k` | Navigate between fields |
| `+/-` or `←/→` | Adjust field value |
| `Enter` | Create scheduled task |
| `n` | Run task now (skip scheduling) |
| `q` / `Esc` | Cancel |

### Mouse Support

| Action | Effect |
|--------|--------|
| **Left click** (parent pane) | Navigate to clicked directory |
| **Left click** (current pane) | Select item, enter if directory |
| **Right click** | Open file with system viewer |
| **Scroll wheel** (file panes) | Navigate up/down |
| **Scroll wheel** (preview pane) | Scroll preview text |
| **Shift+drag** | Select text (terminal native) |

## Configuration

Configuration is stored at `~/.config/clepho/config.toml`:

```toml
db_path = "~/.local/share/clepho/clepho.db"

[llm]
provider = "lmstudio"  # lmstudio, ollama, openai, anthropic
endpoint = "http://127.0.0.1:1234/v1"
model = "gemma-3-4b"
# api_key = "sk-..."  # Required for openai/anthropic

[scanner]
image_extensions = ["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "raw", "cr2", "nef", "arw"]
similarity_threshold = 50  # Hamming distance for perceptual hash (higher = more permissive)

[preview]
image_preview = true
protocol = "auto"  # auto, sixel, kitty, iterm2, halfblocks, none
thumbnail_size = 1024
# external_viewer = "feh"  # Optional: override system default for right-click open

[thumbnails]
path = "~/.cache/clepho/thumbnails"
size = 256

[trash]
path = "~/.local/share/clepho/.trash"
max_age_days = 30
max_size_bytes = 1073741824  # 1GB

[schedule]
check_overdue_on_startup = true  # Prompt for overdue tasks when starting
# default_hours_start = 9        # Optional: default hours of operation
# default_hours_end = 17
```

### LLM Setup

#### LM Studio (default)
1. Install [LM Studio](https://lmstudio.ai/)
2. Download a vision-capable model (e.g., LLaVA, Gemma with vision)
3. Start the local server (default: http://127.0.0.1:1234)
4. Update `config.toml` with your model name

#### Ollama
1. Install [Ollama](https://ollama.ai/)
2. Pull a vision model: `ollama pull llava`
3. Configure:
   ```toml
   [llm]
   provider = "ollama"
   endpoint = "http://127.0.0.1:11434"
   model = "llava"
   ```

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

- File paths, sizes, and modification times
- EXIF metadata (camera, lens, settings, GPS coordinates)
- Complete EXIF data as JSON
- Hash values for duplicate detection (MD5, SHA256, perceptual)
- AI-generated descriptions and tags
- Face detection data and person assignments
- Trash tracking for safe deletion
- Scheduled task queue with status tracking

## Terminal Compatibility

Image previews work best in terminals with graphics protocol support:

| Terminal | Protocol | Quality |
|----------|----------|---------|
| Kitty | Kitty | Excellent |
| iTerm2 | iTerm2 | Excellent |
| WezTerm | Kitty/Sixel | Excellent |
| Konsole | Sixel | Good |
| foot | Sixel | Good |
| xterm | Sixel | Good (if compiled with sixel) |
| Others | Halfblocks | Basic |

Set `protocol = "none"` in config to disable image previews entirely.

## License

GNU GENERAL PUBLIC LICENSE
Version 3, 29 June 2007

# Configuration

Clepho's configuration is stored at `~/.config/clepho/config.toml`. The file is created with defaults on first run.

## Complete Configuration Reference

```toml
[database]
# Backend: "sqlite" (default) or "postgresql"
backend = "sqlite"

# SQLite database path (used when backend = "sqlite")
sqlite_path = "~/.local/share/clepho/clepho.db"

# PostgreSQL connection URL (used when backend = "postgresql")
# Requires building with: cargo build --features postgres
# postgresql_url = "postgresql://user:password@localhost:5432/clepho"

# Connection pool size for PostgreSQL (default: 10)
# pool_size = 10

[llm]
# LLM provider: lmstudio, ollama, openai, anthropic
provider = "lmstudio"

# API endpoint URL
endpoint = "http://127.0.0.1:1234/v1"

# Model name (provider-specific)
model = "gemma-3-4b"

# API key (required for openai/anthropic)
# api_key = "sk-..."

# Embedding model for semantic search (optional)
# embedding_model = "text-embedding-ada-002"

[scanner]
# File extensions to recognize as images
image_extensions = [
    "jpg", "jpeg", "png", "gif", "webp",
    "heic", "heif", "raw", "cr2", "nef", "arw"
]

# Perceptual hash similarity threshold (Hamming distance)
# Lower = stricter matching, Higher = more permissive
# Range: 0-256, Default: 50
similarity_threshold = 50

[preview]
# Enable image previews in the preview pane
image_preview = true

# Graphics protocol: auto, sixel, kitty, iterm2, halfblocks, none
protocol = "auto"

# Maximum dimension for preview thumbnails (pixels)
thumbnail_size = 1024

# External viewer for right-click open (optional)
# If not set, uses system default (xdg-open, open, etc.)
# external_viewer = "feh"

[thumbnails]
# Thumbnail cache directory
path = "~/.cache/clepho/thumbnails"

# Thumbnail size in pixels (square)
size = 256

[trash]
# Trash directory location
path = "~/.local/share/clepho/.trash"

# Auto-delete files older than this (days)
max_age_days = 30

# Maximum trash size in bytes (oldest files deleted first)
max_size_bytes = 1073741824  # 1GB

[schedule]
# Show overdue tasks dialog on startup
check_overdue_on_startup = true

# Default hours of operation for scheduled tasks (optional)
# Tasks will only run between these hours
# default_hours_start = 9
# default_hours_end = 17
```

## Section Details

### Database (`[database]`)

The database stores all metadata, descriptions, face data, and scheduled tasks.

| Setting | Default | Description |
|---------|---------|-------------|
| `backend` | `"sqlite"` | `"sqlite"` or `"postgresql"` |
| `sqlite_path` | `~/.local/share/clepho/clepho.db` | Path to SQLite database file |
| `postgresql_url` | (none) | PostgreSQL connection string |
| `pool_size` | `10` | Connection pool size (PostgreSQL only) |

#### SQLite (default)

- Single file, no server required
- Good for single-user, local use
- **Tip:** Use an SSD location for best performance with large collections

#### PostgreSQL

Requires building with the `postgres` feature flag:

```bash
cargo build --release --features postgres
```

Configure in `config.toml`:
```toml
[database]
backend = "postgresql"
postgresql_url = "postgresql://user:password@localhost:5432/clepho"
pool_size = 10
```

To migrate an existing SQLite database to PostgreSQL:
```bash
clepho --migrate-to-postgres "postgresql://user:password@localhost:5432/clepho"
```

See [database.md](database.md) for more details

### LLM Configuration (`[llm]`)

#### Providers

| Provider | Endpoint | API Key | Notes |
|----------|----------|---------|-------|
| `lmstudio` | `http://127.0.0.1:1234/v1` | No | Local, free |
| `ollama` | `http://127.0.0.1:11434` | No | Local, free |
| `openai` | `https://api.openai.com/v1` | Yes | Cloud, paid |
| `anthropic` | `https://api.anthropic.com` | Yes | Cloud, paid |

#### Model Selection

**LM Studio / Ollama (Vision models):**
- `llava` - Good balance of speed and quality
- `llava:13b` - Better quality, slower
- `bakllava` - Alternative vision model
- `gemma-3-4b` - Fast, lightweight

**OpenAI:**
- `gpt-4-vision-preview` - Best quality
- `gpt-4o` - Fast and capable

### Scanner Configuration (`[scanner]`)

#### Image Extensions

Add or remove extensions based on your collection:

```toml
image_extensions = [
    # Common formats
    "jpg", "jpeg", "png", "gif", "webp",
    # Apple formats
    "heic", "heif",
    # RAW formats
    "raw", "cr2", "nef", "arw", "dng", "orf", "rw2"
]
```

#### Similarity Threshold

Controls perceptual hash matching for duplicate detection:

| Value | Behavior |
|-------|----------|
| 0-20 | Very strict - only near-identical images |
| 20-50 | Moderate - catches resized/recompressed |
| 50-100 | Permissive - catches edited versions |
| 100+ | Very permissive - may have false positives |

### Preview Configuration (`[preview]`)

#### Protocol Selection

| Protocol | Quality | Compatibility |
|----------|---------|---------------|
| `auto` | Varies | Auto-detects best option |
| `kitty` | Excellent | Kitty, WezTerm |
| `iterm2` | Excellent | iTerm2 |
| `sixel` | Good | Konsole, foot, xterm |
| `halfblocks` | Basic | All terminals |
| `none` | N/A | Disables previews |

#### External Viewer

Override system default for right-click open:

```toml
# Use feh for images
external_viewer = "feh"

# Use Eye of GNOME
external_viewer = "eog"

# Use macOS Preview
external_viewer = "open -a Preview"
```

### Trash Configuration (`[trash]`)

The trash system provides safe deletion with automatic cleanup:

- **`max_age_days`**: Files older than this are auto-deleted
- **`max_size_bytes`**: When exceeded, oldest files are deleted first

```toml
[trash]
path = "~/.local/share/clepho/.trash"
max_age_days = 30           # Keep for 30 days
max_size_bytes = 5368709120  # 5GB limit
```

### Schedule Configuration (`[schedule]`)

Controls scheduled task behavior:

```toml
[schedule]
# Prompt for overdue tasks on startup
check_overdue_on_startup = true

# Only run scheduled tasks during work hours
default_hours_start = 9   # 9 AM
default_hours_end = 17    # 5 PM
```

### Keybindings (`[keybindings]`)

All keybindings are configurable. Defaults are aligned with [Yazi](https://yazi-rs.github.io/) file manager where possible.

```toml
[keybindings]
# Navigation (vim-style)
move_down = ["j", "Down"]
move_up = ["k", "Up"]
go_parent = ["h", "Left", "Backspace"]
enter_selected = ["l", "Right", "Enter"]
page_down = ["Ctrl+f"]
page_up = ["Ctrl+b"]

# File operations (Yazi-compatible)
yank_files = ["y", "x"]       # Cut
paste_files = ["p"]           # Paste
delete_files = ["d", "Delete"] # Trash
rename_files = ["r"]          # Rename
toggle_hidden = ["."]         # Toggle hidden files

# Clepho-specific
scan = ["s"]
find_duplicates = ["u"]
describe_with_llm = ["i"]
batch_llm = ["I"]
manage_people = ["P"]
view_trash = ["X"]
open_slideshow = ["S"]
toggle_show_all_files = ["H"]
open_external = ["o"]
```

#### Key Format

| Format | Example | Description |
|--------|---------|-------------|
| Simple | `"j"` | Single character |
| Uppercase | `"G"` | Shift+letter |
| Special | `"Enter"`, `"Space"`, `"Esc"` | Named keys |
| Modifier | `"Ctrl+f"` | With Ctrl/Alt/Shift |

### Library (`[library]`)

Configure a central library location for organizing photos:

```toml
[library]
# Target directory for centralised files
path = "~/Photos/Library"

# Organization pattern: {year}/{month}/{filename}
# Available: {year}, {month}, {day}, {filename}, {ext}
pattern = "{year}/{month}"
```

## Environment Variables

Some settings can be overridden via environment variables:

```bash
# Override config file location
CLEPHO_CONFIG=/path/to/config.toml clepho
```

## Configuration Tips

### Large Collections (100k+ photos)
```toml
[thumbnails]
size = 128  # Smaller thumbnails = faster loading

[preview]
thumbnail_size = 512  # Smaller previews = less memory
```

### Low-Power Devices
```toml
[preview]
protocol = "halfblocks"  # Less CPU-intensive

[scanner]
# Reduce parallel processing by running fewer scans
```

### Network Storage
```toml
# Use local SQLite database even for network photos
[database]
backend = "sqlite"
sqlite_path = "~/.local/share/clepho/clepho.db"

# Or use PostgreSQL for multi-machine access
# [database]
# backend = "postgresql"
# postgresql_url = "postgresql://user:password@dbserver:5432/clepho"

[thumbnails]
# Keep thumbnails local for speed
path = "~/.cache/clepho/thumbnails"
```

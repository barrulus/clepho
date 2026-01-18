# Configuration

Clepho's configuration is stored at `~/.config/clepho/config.toml`. The file is created with defaults on first run.

## Complete Configuration Reference

```toml
# Database location
db_path = "~/.local/share/clepho/clepho.db"

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

### Database (`db_path`)

The SQLite database stores all metadata, descriptions, face data, and scheduled tasks.

- **Default:** `~/.local/share/clepho/clepho.db`
- **Tip:** Use an SSD location for best performance with large collections

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

## Environment Variables

Some settings can be overridden via environment variables:

```bash
# Override database path
CLEPHO_DB_PATH=/path/to/db.sqlite clepho

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
# Use local database even for network photos
db_path = "~/.local/share/clepho/clepho.db"

[thumbnails]
# Keep thumbnails local for speed
path = "~/.cache/clepho/thumbnails"
```

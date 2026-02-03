# Clepho Architecture

## Overview

Clepho is a TUI photo management application built in Rust using the Ratatui framework. It follows a modular architecture with clear separation of concerns.

## Project Structure

```
src/
├── lib.rs               # Library crate (shared config + db modules)
├── main.rs              # TUI binary entry point, terminal setup
├── bin/
│   └── daemon.rs        # Background daemon binary
├── app.rs               # Application state and event loop
├── config.rs            # Configuration management
├── ui/
│   ├── mod.rs           # UI rendering coordinator
│   ├── browser.rs       # Three-column file browser
│   ├── preview.rs       # Preview pane (metadata, descriptions)
│   ├── dialogs.rs       # Help overlay
│   ├── duplicates.rs    # Duplicates view
│   └── status_bar.rs    # Status bar
├── db/
│   ├── mod.rs           # Database enum dispatch wrapper
│   ├── schema.rs        # SQLite schema definition
│   ├── sqlite.rs        # SQLite backend implementation
│   ├── postgres.rs      # PostgreSQL backend (feature-gated)
│   ├── postgres_schema.rs # PostgreSQL schema definition (feature-gated)
│   ├── migrate.rs       # SQLite-to-PostgreSQL migration (feature-gated)
│   ├── similarity.rs    # Duplicate detection types + helpers
│   ├── faces.rs         # Face/person types + helpers
│   ├── embeddings.rs    # Embedding types + helpers
│   ├── albums.rs        # Album/tag types
│   ├── trash.rs         # Trash types
│   └── schedule.rs      # Schedule types
├── scanner/
│   ├── mod.rs           # Scanner coordination
│   ├── discovery.rs     # File discovery (walkdir)
│   ├── metadata.rs      # EXIF extraction
│   └── hashing.rs       # MD5, SHA256, perceptual hashing
└── llm/
    ├── mod.rs           # LLM module exports
    ├── client.rs        # LM Studio API client
    └── queue.rs         # Batch processing queue
```

## Core Components

### Application State (`app.rs`)

The `App` struct holds all application state:
- Current directory and file listings
- Selection indices and scroll offsets
- Mode (Normal, Help, Scanning, Duplicates, LlmProcessing, LlmBatchProcessing)
- Background task receivers (scanning, LLM)
- In-memory caches (LLM descriptions)

### Event Loop

The main event loop in `App::run()`:
1. Checks for background task updates (scan progress, LLM results)
2. Renders the UI
3. Polls for keyboard/mouse events
4. Handles events based on current mode

### UI Rendering (`ui/`)

Rendering is handled by the `ui::render()` function which:
1. Determines current mode
2. Lays out the three-column browser or specialized views
3. Delegates to specific renderers (browser, preview, duplicates)

### Database (`db/`)

The database layer uses an **enum dispatch** pattern to support multiple backends behind a single `Database` API. A `DatabaseInner` enum wraps either `SqliteDb` or `PgDb`, and a `dispatch!` macro forwards all method calls to the active backend. Callers never interact with a specific backend directly.

- **SQLite** (`sqlite.rs`) - Default backend, uses `rusqlite`
- **PostgreSQL** (`postgres.rs`) - Optional, behind `postgres` feature flag, uses `r2d2` connection pooling

Tables:
- `photos` - Core photo metadata, hashes, LLM descriptions
- `people`, `faces`, `face_clusters` - Face detection and recognition
- `embeddings` - CLIP/vision embeddings for semantic search
- `similarity_groups`, `photo_similarity` - Duplicate/similar photo clusters
- `user_tags`, `photo_user_tags`, `albums`, `album_photos` - Organization
- `scans` - Scan history
- `llm_queue` - LLM processing queue
- `scheduled_tasks` - Background task scheduling

### Scanner (`scanner/`)

Background scanning process:
1. Discovers image files using walkdir
2. Extracts EXIF metadata
3. Calculates hashes (MD5, SHA256, perceptual)
4. Stores in database
5. Reports progress via channel

### LLM Integration (`llm/`)

LM Studio integration using ureq HTTP client:
- Single image description (D key)
- Batch processing (P key)
- Descriptions stored in database and cached in memory

## Data Flow

### Scanning Flow
```
User presses 's'
    → start_scan() spawns background thread
    → Scanner walks directory
    → Progress updates sent via channel
    → Main loop receives updates, updates UI
    → Scan complete, photos in database
```

### LLM Description Flow
```
User presses 'D'
    → describe_with_llm() spawns background thread
    → LlmClient sends request to LM Studio
    → Result sent via channel
    → Main loop receives result
    → Description saved to DB and cache
    → Preview pane shows description
```

### Duplicate Detection Flow
```
User presses 'd'
    → find_duplicates() queries database
    → Exact matches (SHA256) + perceptual matches (hamming distance)
    → DuplicatesView created
    → Mode switches to Duplicates
    → User can mark/delete duplicates
```

## Threading Model

- Main thread: UI rendering, event handling
- Background threads: Scanning, LLM requests
- Communication: std::sync::mpsc channels

### Library Crate (`lib.rs`)

The `config` and `db` modules live in the library crate (`src/lib.rs`) so they can be shared between both binaries (TUI and daemon) without duplication.

## Configuration

TOML configuration at `~/.config/clepho/config.toml`:
- Database backend and connection settings
- LLM endpoint and model
- Scanner settings (extensions, similarity threshold)

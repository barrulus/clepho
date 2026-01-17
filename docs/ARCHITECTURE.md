# Clepho Architecture

## Overview

Clepho is a TUI photo management application built in Rust using the Ratatui framework. It follows a modular architecture with clear separation of concerns.

## Project Structure

```
src/
├── main.rs              # Entry point, terminal setup
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
│   ├── mod.rs           # Database connection management
│   ├── schema.rs        # SQLite schema definition
│   └── similarity.rs    # Duplicate detection queries
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

SQLite database with tables for:
- `photos` - Core photo metadata, hashes, LLM descriptions
- `similarity_groups` - Duplicate/similar photo clusters
- `photo_similarity` - Photo-to-group mappings
- `scans` - Scan history
- `llm_queue` - LLM processing queue

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

## Configuration

TOML configuration at `~/.config/clepho/config.toml`:
- Database path
- LLM endpoint and model
- Scanner settings (extensions, similarity threshold)

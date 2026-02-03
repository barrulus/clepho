# Implementation Notes

## Technologies Used

| Component | Library | Version |
|-----------|---------|---------|
| TUI Framework | ratatui | 0.29 |
| Terminal Backend | crossterm | 0.28 |
| Async Runtime | tokio | 1.x |
| Database (SQLite) | rusqlite | 0.32 |
| Database (PostgreSQL) | postgres + r2d2 | 0.19 / 0.8 |
| HTTP Client | ureq | 2.x |
| Image Processing | image | 0.25 |
| EXIF Extraction | kamadak-exif | 0.5 |
| Perceptual Hashing | img_hash | 3.2 |
| Face Detection | ort (ONNX Runtime) | 2.0.0-rc.11 |
| File Walking | walkdir | 2.x |
| Parallel Processing | rayon | 1.10 |

## Key Implementation Decisions

### HTTP Client: ureq vs reqwest

Initially used reqwest, but switched to ureq because:
- reqwest requires TLS libraries even for HTTP connections
- ureq is simpler, blocking, and works without TLS for localhost
- Since LLM calls are already in background threads, async isn't needed

### Perceptual Hashing

Uses `img_hash` crate with its own `image` dependency:
- img_hash uses an older version of the image crate
- Must use `img_hash::image::open()` instead of `image::open()`
- Hamming distance threshold of 10 for similarity detection

### LLM Description Caching

Two-level caching strategy:
1. In-memory HashMap for fast access during session
2. SQLite database for persistence across sessions

When displaying preview:
1. Check memory cache first
2. If not found, check database
3. Cache database result in memory

### Background Processing

All long-running operations use background threads:
- Scanning: walkdir + hash calculation
- LLM requests: HTTP calls to LM Studio

Communication via `std::sync::mpsc` channels:
- Progress updates during processing
- Final results when complete
- Errors propagated to UI

### Borrow Checker Considerations

The `get_llm_description()` method requires `&mut self` because it may:
1. Query the database
2. Insert into the memory cache

This required changing UI rendering to take `&mut App` and cloning the selected entry to avoid borrow conflicts.

## Database Architecture

The database layer uses an **enum dispatch** pattern (`src/db/mod.rs`) to support SQLite and PostgreSQL behind a unified `Database` API:

```
Database
  └── DatabaseInner (enum)
        ├── Sqlite(SqliteDb)     ← src/db/sqlite.rs
        └── Postgres(PgDb)       ← src/db/postgres.rs (feature-gated)
```

A `dispatch!` macro forwards all ~98 public methods to the active backend variant. External code calls `Database` methods without knowing which backend is active.

PostgreSQL support is behind the `postgres` Cargo feature flag. The `config` and `db` modules live in the library crate (`src/lib.rs`) so both binaries (TUI and daemon) share them.

See `src/db/schema.rs` (SQLite) and `src/db/postgres_schema.rs` (PostgreSQL) for the full schema. Key tables:

### photos
- Core metadata (path, size, timestamps)
- Image info (dimensions, format)
- EXIF data (camera, settings, GPS)
- Hashes (MD5, SHA256, perceptual)
- LLM content (description, tags)

### similarity_groups
- Groups of duplicate/similar photos
- Type: 'exact' or 'perceptual'

## Known Limitations

1. **HEIC support** - Requires system libheif for full support
2. **PostgreSQL requires feature flag** - Not included in default build

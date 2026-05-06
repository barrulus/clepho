# Foundation + Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace clepho's data layer and pipeline with a typed-facet schema, per-photo state machine, provenance-aware writes, and a 5-stage scheduler (scan/exif/thumb/llm/index) — leaving the TUI mostly intact except for new pipeline-status surfaces and reset/reprocess dialogs. Faces stage stubbed (Plan 2 implements).

**Architecture:** Fresh schema replaces the old `photos.tags` JSON column and parallel `user_tags` system with typed facet tables (`photo_objects`, `photo_user_tags`, `photo_events`, `albums`, `managed_folders`, `folder_prompts`, `pipeline_events`, `rejected_suggestions`, plus `faces`/`people` tables defined-but-unused). Each photo carries six stage timestamp columns (`scan_done_at`, `exif_done_at`, `thumb_done_at`, `llm_done_at`, `faces_done_at`, `index_done_at`); the scheduler skips photos where the relevant stage is done. Provenance enforcement (`source` + `confirmed_at` columns on every facet relation) lives at the DB layer as a single function. Daemon and TUI both invoke the same scheduler. Errors land in `pipeline_events` (DB) and a JSONL log file (disk).

**Tech Stack:** Rust 2021, `rusqlite` (bundled SQLite), `postgres`/`r2d2` (optional Postgres), `tokio` async, `tracing` for logs, `anyhow`/`thiserror` for errors, `chrono` for time, `serde`/`serde_json` for filter JSON, `insta` (new) for snapshot tests, `proptest` (new) for property tests, `mockall` (new) for mocks, `rayon` for parallel work, `ratatui` 0.29 for TUI.

**Spec reference:** `docs/superpowers/specs/2026-05-04-pipeline-tagging-alignment-design.md`. Each task cites the relevant section.

---

## File Structure

**New files (created in this plan):**

| File | Responsibility |
|---|---|
| `src/db/schema_v2.rs` | Fresh DDL for the new schema (replaces `schema.rs`'s `SCHEMA` const) |
| `src/db/provenance.rs` | The provenance contract function + write helpers (Section 6.2 of spec) |
| `src/db/clock.rs` | `Clock` trait for deterministic time injection |
| `src/db/facets.rs` | Read/write helpers for `photo_objects`, `photo_user_tags`, `photo_events` |
| `src/db/managed_folders.rs` | CRUD for `managed_folders` and `folder_prompts` |
| `src/db/pipeline_events.rs` | Append/query helpers for `pipeline_events` |
| `src/db/filter_eval.rs` | Smart-album filter JSON evaluator |
| `src/pipeline/mod.rs` | Pipeline module root, `Stage` trait, public API |
| `src/pipeline/scheduler.rs` | Worker pool dispatcher, per-stage queues |
| `src/pipeline/circuit_breaker.rs` | Per-stage consecutive-failure tracking |
| `src/pipeline/log.rs` | JSONL structured log appender |
| `src/pipeline/stages/scan.rs` | Filesystem walk + hash + basic metadata |
| `src/pipeline/stages/exif.rs` | EXIF extraction → `photos.taken_at, gps_*, camera_*` |
| `src/pipeline/stages/thumb.rs` | Thumbnail generation (refactored from existing scanner) |
| `src/pipeline/stages/llm.rs` | LLM stage writing description (provenance) + `photo_objects` |
| `src/pipeline/stages/index.rs` | Index stage (smart-album recompute, FTS, centroid placeholder) |
| `src/pipeline/stages/mod.rs` | Stage registry + factory |
| `src/ui/pipeline_status.rs` | Pipeline Status screen (replaces existing task list view) |
| `src/ui/reprocess_dialog.rs` | Force-reprocess dialog with stage selection + provenance preflight |
| `src/ui/reset_db_dialog.rs` | One-time reset dialog when old schema detected |
| `tests/provenance_contract.rs` | Table-driven contract enforcement tests |
| `tests/skip_if_done.rs` | Skip-if-done integration test |
| `tests/force_reprocess.rs` | Force-reprocess preserves user/confirmed values |
| `tests/stage_idempotency.rs` | Running stages twice is a no-op |
| `tests/resume_after_crash.rs` | Drop stage future before commit, restart, no duplicates |
| `tests/circuit_breaker.rs` | Three same-class failures pause stage |
| `tests/filter_eval.rs` | Filter evaluator + property tests |
| `tests/fixtures/photos/manifest.json` | Fixture inventory |
| `tests/fixtures/photos/*.jpg` | Small JPEGs with curated EXIF |
| `tests/fixtures/llm_responses/*.json` | Canned LLM response bodies |

**Files modified:**

| File | Modification |
|---|---|
| `Cargo.toml` | Add `insta`, `proptest`, `mockall`, `notify` dev-dependencies |
| `src/lib.rs` | Register new `pipeline` module |
| `src/db/mod.rs` | Replace SCHEMA reference, add new method signatures |
| `src/db/sqlite.rs` | Implement new schema methods |
| `src/db/postgres.rs` (feature-gated) | Implement new schema methods |
| `src/db/schema.rs` | Replaced contents — points to `schema_v2.rs` |
| `src/db/migrate.rs` | Detect old schema → trigger reset flow (no auto-migration) |
| `src/llm/client.rs` | `describe_and_tag_image` returns `(description, Vec<String>)` already; surface used by new LLM stage |
| `src/config.rs` | Add `[pipeline]`, `[logging]` sections; rename `batch_concurrency` |
| `src/bin/daemon.rs` | Drive scheduler instead of inline batch logic |
| `src/app.rs` | Add `R`, `Shift+R`, `M` key handlers; reset-DB on startup; `T` shows new screen |
| `src/ui/status_bar.rs` | New pipeline progress format `[Px:42%]` |
| `src/ui/mod.rs` | Wire new dialogs and pipeline-status screen |
| `src/logging.rs` | Add JSONL appender layer |
| `.gitignore` | Add `tests/fixtures/*.cache/` if needed |

**Files deleted:**

| File | Reason |
|---|---|
| `src/llm/queue.rs` | Replaced by scheduler |
| `src/db/schedule.rs` (eventually) | Folded into `managed_folders` — but kept for now if the existing `schedule_dialog.rs` still uses it; flag in Task 22 |

---

## Test Fixtures

`tests/fixtures/photos/` ships 8-10 small JPEGs (≤200KB each) with hand-curated EXIF. Listed in `manifest.json`:

```json
{
  "photos": [
    {"file": "rome_2024_06_12.jpg", "taken_at": "2024-06-12T19:23:45+02:00",
     "gps": {"lat": 41.8902, "lon": 12.4922}, "camera": "Sony A7iv",
     "expected_objects": ["sunset", "river"]},
    {"file": "florence_2024_06_15.jpg", "taken_at": "2024-06-15T14:10:00+02:00",
     "gps": {"lat": 43.7696, "lon": 11.2558}, "camera": "Sony A7iv",
     "expected_objects": ["building", "architecture"]},
    {"file": "no_exif.jpg", "taken_at": null, "gps": null, "camera": null,
     "expected_objects": []},
    {"file": "no_gps.jpg", "taken_at": "2023-01-01T12:00:00Z", "gps": null,
     "camera": "iPhone 13", "expected_objects": []},
    ...
  ]
}
```

Source photos: synthesise tiny JPEGs via Rust `image` crate at fixture-prep time (run once, commit results), or use Creative Commons stock. Total size ≤2 MB.

`tests/fixtures/llm_responses/`:
- `valid.json` — `{"description": "...", "tags": ["a","b"]}`
- `code_fenced.json` — ` ```json\n{...}\n``` ` wrapper
- `legacy_tags.txt` — old `TAGS:` delimiter format
- `malformed.txt` — broken response
- `empty_tags.json` — `{"description": "...", "tags": []}`

---

## Tasks

### Task 1: Add dev-dependencies and feature flags

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add new dev-dependencies**

Add this block under `[dev-dependencies]` (create the section if missing):

```toml
[dev-dependencies]
insta = { version = "1", features = ["json", "redactions"] }
proptest = "1"
mockall = "0.13"
tempfile = "3"
notify = "6"     # filesystem watcher; tested in Task 21
serial_test = "3"  # serialise tests that touch shared resources
```

- [ ] **Step 2: Verify build still passes**

Run: `cargo build --all-targets`
Expected: build succeeds with new deps downloaded.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "Add dev-dependencies for testing infrastructure"
```

---

### Task 2: `Clock` trait for deterministic time injection

**Files:**
- Create: `src/db/clock.rs`
- Modify: `src/db/mod.rs` (add `pub mod clock;` and re-export `Clock`)

- [ ] **Step 1: Write the failing test**

Create `src/db/clock.rs`:

```rust
//! Clock trait: every place that stamps `now()` (confirmed_at, taken_at fallbacks,
//! pipeline_events occurred_at, etc.) takes a `&dyn Clock` so tests are deterministic.

use chrono::{DateTime, TimeZone, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct FixedClock(DateTime<Utc>);

impl FixedClock {
    pub fn new(ts: DateTime<Utc>) -> Self { Self(ts) }
    pub fn iso(s: &str) -> Self {
        Self(DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc))
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> { self.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_set_time() {
        let c = FixedClock::iso("2026-05-04T12:00:00Z");
        assert_eq!(c.now().to_rfc3339(), "2026-05-04T12:00:00+00:00");
    }

    #[test]
    fn system_clock_advances() {
        let c = SystemClock;
        let a = c.now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = c.now();
        assert!(b > a);
    }
}
```

- [ ] **Step 2: Wire into `src/db/mod.rs`**

Add near the other `pub mod ...` declarations:

```rust
pub mod clock;
pub use clock::{Clock, SystemClock, FixedClock};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib clock::tests`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/db/clock.rs src/db/mod.rs
git commit -m "Add Clock trait for deterministic time injection"
```

---

### Task 3: New schema (`schema_v2.rs`) — definitions only

This creates the DDL constant. Wiring (drop+recreate vs migrate) is Task 4.

**Files:**
- Create: `src/db/schema_v2.rs`

Spec reference: §3 (Data Model).

- [ ] **Step 1: Create file with full DDL**

```rust
//! Fresh schema for clepho v2. The migration story is "drop + recreate" because
//! existing data is experimental (see spec §10.1). Faces tables are defined here
//! but unused in Plan 1 (Plan 2 wires the FaceEngine).

pub const SCHEMA_V2: &str = r#"
-- ============================================================================
-- photos: per-photo state row (singleton facets + pipeline state)
-- ============================================================================
CREATE TABLE IF NOT EXISTS photos (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT NOT NULL UNIQUE,
    file_hash       TEXT,
    file_size       INTEGER,
    width           INTEGER,
    height          INTEGER,
    mime            TEXT,

    -- singleton facets
    taken_at        TEXT,
    gps_lat         REAL,
    gps_lon         REAL,
    gps_alt         REAL,
    gps_place_label TEXT,                       -- reserved for future reverse-geocoding
    camera_make     TEXT,
    camera_model    TEXT,
    camera_lens     TEXT,

    -- description with provenance
    description              TEXT,
    description_source       TEXT CHECK (description_source IN ('ai','user','ai_edited')),
    description_confirmed_at TEXT,

    -- pipeline state (skip-if-done gates on these)
    scan_done_at    TEXT,
    exif_done_at    TEXT,
    thumb_done_at   TEXT,
    llm_done_at     TEXT,
    faces_done_at   TEXT,
    index_done_at   TEXT,

    -- latest error per stage (cleared on successful re-run)
    scan_error      TEXT,
    exif_error      TEXT,
    thumb_error     TEXT,
    llm_error       TEXT,
    faces_error     TEXT,
    index_error     TEXT,

    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_photos_path        ON photos(path);
CREATE INDEX IF NOT EXISTS idx_photos_scan_pend   ON photos(id) WHERE scan_done_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_photos_exif_pend   ON photos(id) WHERE exif_done_at IS NULL AND scan_done_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_photos_thumb_pend  ON photos(id) WHERE thumb_done_at IS NULL AND exif_done_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_photos_llm_pend    ON photos(id) WHERE llm_done_at IS NULL AND exif_done_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_photos_faces_pend  ON photos(id) WHERE faces_done_at IS NULL AND exif_done_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_photos_index_pend  ON photos(id) WHERE index_done_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_photos_taken_at    ON photos(taken_at);
CREATE INDEX IF NOT EXISTS idx_photos_gps         ON photos(gps_lat, gps_lon);

-- ============================================================================
-- multi-valued facet tables (each has source + confirmed_at)
-- ============================================================================
CREATE TABLE IF NOT EXISTS objects (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS photo_objects (
    photo_id     INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    object_id    INTEGER NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
    source       TEXT NOT NULL CHECK (source IN ('ai','user')),
    confirmed_at TEXT,
    PRIMARY KEY (photo_id, object_id)
);

CREATE INDEX IF NOT EXISTS idx_photo_objects_obj ON photo_objects(object_id);

CREATE TABLE IF NOT EXISTS user_tags (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    color      TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS photo_user_tags (
    photo_id     INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    tag_id       INTEGER NOT NULL REFERENCES user_tags(id) ON DELETE CASCADE,
    source       TEXT NOT NULL CHECK (source IN ('ai','user')) DEFAULT 'user',
    confirmed_at TEXT,
    PRIMARY KEY (photo_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_photo_user_tags_tag ON photo_user_tags(tag_id);

CREATE TABLE IF NOT EXISTS events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    description    TEXT,
    cover_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL,
    date_start     TEXT,
    date_end       TEXT,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS photo_events (
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (photo_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_photo_events_event ON photo_events(event_id);

-- ============================================================================
-- faces / people (defined here, used in Plan 2)
-- ============================================================================
CREATE TABLE IF NOT EXISTS people (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT,
    embedding     BLOB,
    cover_face_id INTEGER,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS faces (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id             INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    bbox_x               REAL NOT NULL,
    bbox_y               REAL NOT NULL,
    bbox_w               REAL NOT NULL,
    bbox_h               REAL NOT NULL,
    embedding            BLOB,
    person_id            INTEGER REFERENCES people(id) ON DELETE SET NULL,
    source               TEXT NOT NULL CHECK (source IN ('ai','user')),
    confirmed_at         TEXT,
    detection_confidence REAL,
    match_similarity     REAL,
    created_at           TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_faces_photo  ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);

-- ============================================================================
-- rejected_suggestions: per-photo per-value AI rejections
-- ============================================================================
CREATE TABLE IF NOT EXISTS rejected_suggestions (
    photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    facet       TEXT NOT NULL CHECK (facet IN ('object','person','user_tag')),
    value       TEXT NOT NULL,
    rejected_at TEXT NOT NULL,
    PRIMARY KEY (photo_id, facet, value)
);

-- ============================================================================
-- albums (manual + smart in one table)
-- ============================================================================
CREATE TABLE IF NOT EXISTS albums (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    description    TEXT,
    cover_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('manual','smart')),
    filter_json    TEXT,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS album_photos (
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    position INTEGER,
    PRIMARY KEY (album_id, photo_id)
);

CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);

-- ============================================================================
-- managed_folders + folder_prompts (replaces old directory_prompts)
-- ============================================================================
CREATE TABLE IF NOT EXISTS managed_folders (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT NOT NULL UNIQUE,
    schedule_cron   TEXT,
    paused          INTEGER NOT NULL DEFAULT 0,
    last_run_at     TEXT,
    faces_disabled  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS folder_prompts (
    path          TEXT PRIMARY KEY,
    custom_prompt TEXT NOT NULL,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- ============================================================================
-- pipeline_events: audit/observability log (DB-side companion of JSONL file)
-- ============================================================================
CREATE TABLE IF NOT EXISTS pipeline_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at  TEXT NOT NULL,
    level        TEXT NOT NULL CHECK (level IN ('info','warn','error')),
    stage        TEXT,
    folder       TEXT,
    photo_id     INTEGER,
    error_class  TEXT,
    message      TEXT NOT NULL,
    context_json TEXT,
    resolved_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_pipeline_events_unresolved
  ON pipeline_events(occurred_at) WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_pipeline_events_stage_class
  ON pipeline_events(stage, error_class);

-- ============================================================================
-- embeddings: text-embedding store (kept from current schema for semantic search)
-- ============================================================================
CREATE TABLE IF NOT EXISTS embeddings (
    photo_id  INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    kind      TEXT NOT NULL,
    model     TEXT,
    dims      INTEGER NOT NULL,
    vector    BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- ============================================================================
-- schema_version: tracks which schema generation we're on
-- ============================================================================
CREATE TABLE IF NOT EXISTS schema_version (
    version    INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO schema_version(version) VALUES (2);
"#;

pub const SCHEMA_GENERATION: i64 = 2;
```

- [ ] **Step 2: Verify it compiles as a const**

Run: `cargo build --lib`
Expected: builds (it's just a string).

- [ ] **Step 3: Commit**

```bash
git add src/db/schema_v2.rs
git commit -m "Add v2 schema DDL (typed facets + pipeline state)"
```

---

### Task 4: Schema reset detection + migration entry point

When the daemon/TUI starts and finds a database with schema generation `< 2`, it must NOT auto-migrate. Instead, present the reset dialog (Task 19) on first interactive run; for headless daemon, refuse to start with a clear error message.

**Files:**
- Modify: `src/db/migrate.rs`
- Modify: `src/db/sqlite.rs` (call `detect_schema_generation` on open)
- Modify: `src/db/mod.rs` (re-export)

Spec reference: §10.1.

- [ ] **Step 1: Define schema-generation detection**

Add to `src/db/migrate.rs` (or create top of file if currently feature-gated):

```rust
//! Schema generation detection. clepho v2 (this plan) uses generation 2.
//! Anything below = legacy schema; user must explicitly opt-in to reset (Task 19).

use anyhow::Result;
use rusqlite::Connection;

pub const CURRENT_GENERATION: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaState {
    Empty,           // brand-new DB, no tables yet
    Legacy,          // has 'photos' but no 'schema_version' or version < 2
    Current,         // version = CURRENT_GENERATION
    Newer(i64),      // version > CURRENT_GENERATION (downgrade case)
}

pub fn detect_schema_state(conn: &Connection) -> Result<SchemaState> {
    // Does any table exist?
    let any_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%')",
        [],
        |r| r.get(0),
    )?;
    if !any_table {
        return Ok(SchemaState::Empty);
    }

    // schema_version table present?
    let has_version: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !has_version {
        return Ok(SchemaState::Legacy);
    }

    let version: i64 = conn.query_row(
        "SELECT MAX(version) FROM schema_version",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    Ok(match version.cmp(&CURRENT_GENERATION) {
        std::cmp::Ordering::Less    => SchemaState::Legacy,
        std::cmp::Ordering::Equal   => SchemaState::Current,
        std::cmp::Ordering::Greater => SchemaState::Newer(version),
    })
}

pub fn apply_v2_schema(conn: &Connection) -> Result<()> {
    use crate::db::schema_v2::SCHEMA_V2;
    conn.execute_batch(SCHEMA_V2)?;
    Ok(())
}

/// Drop ALL existing tables and indices, then apply v2 schema. Destructive.
/// Caller must obtain user consent before invoking (see ResetDbDialog, Task 19).
pub fn reset_to_v2(conn: &Connection) -> Result<()> {
    let tables: Vec<String> = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    )?.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;

    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    for t in &tables {
        conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", t), [])?;
    }
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    apply_v2_schema(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_mem() -> Connection { Connection::open_in_memory().unwrap() }

    #[test]
    fn empty_db_detected_as_empty() {
        let c = open_mem();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Empty);
    }

    #[test]
    fn legacy_db_detected_as_legacy() {
        let c = open_mem();
        c.execute_batch("CREATE TABLE photos (id INTEGER PRIMARY KEY);").unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Legacy);
    }

    #[test]
    fn v2_db_detected_as_current() {
        let c = open_mem();
        apply_v2_schema(&c).unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);
    }

    #[test]
    fn reset_drops_legacy_and_applies_v2() {
        let c = open_mem();
        c.execute_batch("CREATE TABLE photos (id INTEGER PRIMARY KEY, tags TEXT);").unwrap();
        c.execute("INSERT INTO photos(tags) VALUES ('legacy')", []).unwrap();

        reset_to_v2(&c).unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);

        // The new photos table doesn't have a 'tags' column — verify by querying.
        let cols: Vec<String> = c.prepare("PRAGMA table_info(photos)").unwrap()
            .query_map([], |r| r.get::<_, String>(1)).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();
        assert!(cols.contains(&"description_source".to_string()));
        assert!(!cols.contains(&"tags".to_string()));
    }
}
```

- [ ] **Step 2: Make `migrate` module unconditional**

In `src/db/mod.rs`, change:

```rust
#[cfg(feature = "postgres")]
pub mod migrate;
```

to:

```rust
pub mod migrate;
pub use migrate::{SchemaState, detect_schema_state, apply_v2_schema, reset_to_v2};
```

Move any postgres-specific migration code into a separate `pub mod migrate_postgres;` if needed.

- [ ] **Step 3: Run tests**

Run: `cargo test --lib migrate::tests`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/db/migrate.rs src/db/mod.rs
git commit -m "Detect legacy schema and provide reset_to_v2 helper"
```

---

### Task 5: Provenance contract function

The single load-bearing rule of the system. One function, with table-driven tests covering every cell of the §6.2 matrix.

**Files:**
- Create: `src/db/provenance.rs`
- Modify: `src/db/mod.rs` (`pub mod provenance;`)

Spec reference: §6.

- [ ] **Step 1: Write the contract enum and types**

Create `src/db/provenance.rs`:

```rust
//! The provenance contract (spec §6).
//!
//! The pipeline writes only where `source = 'ai' AND confirmed_at IS NULL`.
//! Users write anywhere; their writes set `source = 'user'` and `confirmed_at = now()`.
//!
//! This module is the SINGLE place that translates write intents into SQL effects.
//! All facet-write paths (LLM stage, user edits, bulk ops) MUST go through here.

use crate::db::clock::Clock;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor { Pipeline, User }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacetTable {
    PhotoObjects,    // photo_objects
    PhotoUserTags,   // photo_user_tags
    Faces,           // faces (for Plan 2; included now for completeness)
}

impl FacetTable {
    fn name(self) -> &'static str {
        match self {
            FacetTable::PhotoObjects   => "photo_objects",
            FacetTable::PhotoUserTags  => "photo_user_tags",
            FacetTable::Faces          => "faces",
        }
    }
    fn id_col(self) -> &'static str {
        match self {
            FacetTable::PhotoObjects   => "object_id",
            FacetTable::PhotoUserTags  => "tag_id",
            FacetTable::Faces          => "id",
        }
    }
}

/// One write outcome — what happened, for callers that want to log/test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Inserted,         // new row written
    UpdatedConfirmed, // row existed (ai, unconfirmed) and was confirmed by user
    SkippedExisting,  // row exists with confirmed/user provenance, no-op
    SkippedRejected,  // value is in rejected_suggestions, no-op
    Idempotent,       // row exists with same source, no-op
}

/// Pipeline-side write of an AI suggestion. Honours rejected_suggestions and
/// the contract: never overwrites confirmed/user rows.
pub fn pipeline_write_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    rejected_facet_key: &str,    // "object" | "user_tag" | "person"
    rejected_facet_value: &str,  // the human-readable name to check rejections by
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    // 1. Check rejected_suggestions
    let is_rejected: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM rejected_suggestions
                         WHERE photo_id = ?1 AND facet = ?2 AND value = ?3)",
        params![photo_id, rejected_facet_key, rejected_facet_value],
        |r| r.get(0),
    )?;
    if is_rejected { return Ok(WriteOutcome::SkippedRejected); }

    // 2. Check existing row state
    let existing: Option<(String, Option<String>)> = conn.query_row(
        &format!(
            "SELECT source, confirmed_at FROM {} WHERE photo_id = ?1 AND {} = ?2",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional()?;

    match existing {
        Some((source, confirmed_at)) => {
            if source == "user" || confirmed_at.is_some() {
                Ok(WriteOutcome::SkippedExisting)
            } else {
                // already (ai, NULL) — idempotent rewrite
                Ok(WriteOutcome::Idempotent)
            }
        }
        None => {
            conn.execute(
                &format!(
                    "INSERT INTO {} (photo_id, {}, source, confirmed_at) VALUES (?1, ?2, 'ai', NULL)",
                    table.name(), table.id_col()
                ),
                params![photo_id, value_id],
            )?;
            Ok(WriteOutcome::Inserted)
        }
    }
}

/// User adds a value manually. Sets source='user', confirmed_at=now().
pub fn user_add_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let now = clock.now().to_rfc3339();

    // If already present, just confirm/upgrade
    let existing: Option<String> = conn.query_row(
        &format!(
            "SELECT source FROM {} WHERE photo_id = ?1 AND {} = ?2",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id],
        |r| r.get(0),
    ).optional()?;

    match existing {
        Some(_) => {
            conn.execute(
                &format!(
                    "UPDATE {} SET source='user', confirmed_at=?3
                     WHERE photo_id=?1 AND {}=?2",
                    table.name(), table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            Ok(WriteOutcome::UpdatedConfirmed)
        }
        None => {
            conn.execute(
                &format!(
                    "INSERT INTO {} (photo_id, {}, source, confirmed_at) VALUES (?1, ?2, 'user', ?3)",
                    table.name(), table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            Ok(WriteOutcome::Inserted)
        }
    }
}

/// User accepts an AI suggestion: keeps source='ai' but sets confirmed_at.
pub fn user_confirm_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let now = clock.now().to_rfc3339();
    let updated = conn.execute(
        &format!(
            "UPDATE {} SET confirmed_at=?3
             WHERE photo_id=?1 AND {}=?2 AND source='ai' AND confirmed_at IS NULL",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id, now],
    )?;
    if updated == 0 { bail!("nothing to confirm: row absent or not in (ai,NULL) state"); }
    Ok(WriteOutcome::UpdatedConfirmed)
}

/// User removes a value (DELETE). Re-suggestable next pipeline run.
pub fn user_remove_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
) -> Result<()> {
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE photo_id=?1 AND {}=?2",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id],
    )?;
    Ok(())
}

/// User rejects an AI suggestion: DELETE + INSERT into rejected_suggestions.
/// `value` is the human-readable name (object name, tag name, person name).
pub fn user_reject_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    rejected_facet_key: &str,
    rejected_facet_value: &str,
    clock: &dyn Clock,
) -> Result<()> {
    let now = clock.now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!(
            "DELETE FROM {} WHERE photo_id=?1 AND {}=?2",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO rejected_suggestions(photo_id, facet, value, rejected_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![photo_id, rejected_facet_key, rejected_facet_value, now],
    )?;
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 2: Wire into mod.rs**

In `src/db/mod.rs`, add:

```rust
pub mod provenance;
pub use provenance::{Actor, FacetTable, WriteOutcome,
    pipeline_write_facet, user_add_facet, user_confirm_facet,
    user_remove_facet, user_reject_facet};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --lib`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add src/db/provenance.rs src/db/mod.rs
git commit -m "Add provenance contract write helpers"
```

---

### Task 6: Provenance contract — exhaustive table-driven tests

The contract matrix from spec §6.2, every cell. This is the keystone test suite.

**Files:**
- Create: `tests/provenance_contract.rs`

- [ ] **Step 1: Set up test scaffolding**

Create `tests/provenance_contract.rs`:

```rust
//! Spec §6.2 enforcement — every operation × every starting state → expected outcome.
//! If any cell of this matrix is wrong, the whole alignment effort breaks.

use clepho::db::{
    apply_v2_schema, FixedClock, FacetTable, WriteOutcome,
    pipeline_write_facet, user_add_facet, user_confirm_facet,
    user_remove_facet, user_reject_facet,
};
use rusqlite::Connection;

fn setup() -> (Connection, FixedClock) {
    let conn = Connection::open_in_memory().unwrap();
    apply_v2_schema(&conn).unwrap();
    // seed: one photo, one object value
    conn.execute("INSERT INTO photos(path) VALUES ('p1.jpg')", []).unwrap();
    conn.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();
    let clock = FixedClock::iso("2026-05-04T12:00:00Z");
    (conn, clock)
}

fn row_state(conn: &Connection) -> Option<(String, Option<String>)> {
    conn.query_row(
        "SELECT source, confirmed_at FROM photo_objects WHERE photo_id=1 AND object_id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).ok()
}

// --- Pipeline writes -------------------------------------------------------

#[test]
fn pipeline_write_to_empty_inserts_ai_unconfirmed() {
    let (c, clk) = setup();
    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1,
        "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
    assert_eq!(row_state(&c), Some(("ai".into(), None)));
}

#[test]
fn pipeline_write_to_existing_ai_unconfirmed_is_idempotent() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Idempotent);
    assert_eq!(row_state(&c), Some(("ai".into(), None)));
}

#[test]
fn pipeline_write_skipped_when_existing_user_row() {
    let (c, clk) = setup();
    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedExisting);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert!(conf.is_some());
}

#[test]
fn pipeline_write_skipped_when_existing_ai_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedExisting);
}

#[test]
fn pipeline_write_skipped_when_value_rejected() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert!(row_state(&c).is_none());

    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedRejected);
    assert!(row_state(&c).is_none());
}

// --- User add / confirm / remove / reject ---------------------------------

#[test]
fn user_add_to_empty_inserts_user_confirmed() {
    let (c, clk) = setup();
    let outcome = user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert_eq!(conf.unwrap(), "2026-05-04T12:00:00+00:00");
}

#[test]
fn user_add_existing_ai_upgrades_to_user_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome = user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::UpdatedConfirmed);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert!(conf.is_some());
}

#[test]
fn user_confirm_promotes_ai_to_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome = user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::UpdatedConfirmed);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "ai");
    assert!(conf.is_some());
}

#[test]
fn user_confirm_fails_on_missing_row() {
    let (c, clk) = setup();
    let r = user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk);
    assert!(r.is_err(), "expected error for confirm of missing row");
}

#[test]
fn user_remove_deletes_row() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_remove_facet(&c, FacetTable::PhotoObjects, 1, 1).unwrap();
    assert!(row_state(&c).is_none());
}

#[test]
fn user_remove_does_not_record_rejection() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_remove_facet(&c, FacetTable::PhotoObjects, 1, 1).unwrap();

    let rejected_count: i64 = c.query_row(
        "SELECT COUNT(*) FROM rejected_suggestions", [], |r| r.get(0)).unwrap();
    assert_eq!(rejected_count, 0);

    // pipeline can re-suggest
    let outcome = pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
}

#[test]
fn user_reject_deletes_and_records_rejection() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert!(row_state(&c).is_none());

    let cnt: i64 = c.query_row(
        "SELECT COUNT(*) FROM rejected_suggestions WHERE photo_id=1 AND facet='object' AND value='sunset'",
        [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 1);
}

#[test]
fn user_add_after_reject_clears_rejection_implicitly_via_replace() {
    // Spec §6.4: "If the user later adds the rejected value back manually,
    // the rejected_suggestions row for that combo is deleted."
    // Our `user_add_facet` does NOT currently clear rejections — flag for fix.
    let (c, clk) = setup();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();

    // Expected: rejection cleared (spec).
    let cnt: i64 = c.query_row(
        "SELECT COUNT(*) FROM rejected_suggestions WHERE photo_id=1 AND facet='object'",
        [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 0, "user re-adding a rejected value must clear the rejection (spec §6.4)");
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --test provenance_contract`
Expected: most pass, but `user_add_after_reject_clears_rejection_implicitly_via_replace` FAILS — exposing a bug in `user_add_facet` (it doesn't clear rejections).

- [ ] **Step 3: Fix `user_add_facet` to clear rejection rows**

In `src/db/provenance.rs`, modify `user_add_facet` to wrap its work in a transaction and clear rejections by name. Since `user_add_facet` only knows the value_id (not the human name), it needs to look up the name from the right facet table:

```rust
/// User adds a value manually. Sets source='user', confirmed_at=now().
/// Per spec §6.4, also clears any matching rejected_suggestions row.
pub fn user_add_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let now = clock.now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;

    // Look up human name + rejected_suggestions facet key for this table
    let (rejected_key, value_name) = match table {
        FacetTable::PhotoObjects => {
            let n: String = tx.query_row(
                "SELECT name FROM objects WHERE id=?1", params![value_id], |r| r.get(0))?;
            ("object", n)
        }
        FacetTable::PhotoUserTags => {
            let n: String = tx.query_row(
                "SELECT name FROM user_tags WHERE id=?1", params![value_id], |r| r.get(0))?;
            ("user_tag", n)
        }
        FacetTable::Faces => {
            // Faces don't go through this helper for person assignment in v1; flag.
            ("person", String::new())
        }
    };

    // Clear any matching rejection
    if !value_name.is_empty() {
        tx.execute(
            "DELETE FROM rejected_suggestions WHERE photo_id=?1 AND facet=?2 AND value=?3",
            params![photo_id, rejected_key, value_name],
        )?;
    }

    let existing: Option<String> = tx.query_row(
        &format!(
            "SELECT source FROM {} WHERE photo_id = ?1 AND {} = ?2",
            table.name(), table.id_col()
        ),
        params![photo_id, value_id],
        |r| r.get(0),
    ).optional()?;

    let outcome = match existing {
        Some(_) => {
            tx.execute(
                &format!(
                    "UPDATE {} SET source='user', confirmed_at=?3
                     WHERE photo_id=?1 AND {}=?2",
                    table.name(), table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            WriteOutcome::UpdatedConfirmed
        }
        None => {
            tx.execute(
                &format!(
                    "INSERT INTO {} (photo_id, {}, source, confirmed_at) VALUES (?1, ?2, 'user', ?3)",
                    table.name(), table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            WriteOutcome::Inserted
        }
    };
    tx.commit()?;
    Ok(outcome)
}
```

- [ ] **Step 4: Re-run tests**

Run: `cargo test --test provenance_contract`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/db/provenance.rs tests/provenance_contract.rs
git commit -m "Test provenance contract exhaustively; fix add-after-reject"
```

---

### Task 7: `pipeline_events` writer

**Files:**
- Create: `src/db/pipeline_events.rs`
- Modify: `src/db/mod.rs` (add `pub mod pipeline_events;`)

Spec reference: §8.2.

- [ ] **Step 1: Define types and writer**

Create `src/db/pipeline_events.rs`:

```rust
//! pipeline_events: DB-backed observability log (spec §8.2).
//! Companion of the JSONL log file; capped at 10k rows with auto-rotation.

use crate::db::clock::Clock;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel { Info, Warn, Error }

impl EventLevel {
    fn as_str(self) -> &'static str {
        match self { Self::Info => "info", Self::Warn => "warn", Self::Error => "error" }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineEvent {
    pub level: EventLevel,
    pub stage: Option<String>,
    pub folder: Option<String>,
    pub photo_id: Option<i64>,
    pub error_class: Option<String>,
    pub message: String,
    pub context_json: Option<String>,
}

/// Append an event. Rotates oldest rows when count exceeds 10_000.
pub fn append(conn: &Connection, ev: &PipelineEvent, clock: &dyn Clock) -> Result<i64> {
    let now = clock.now().to_rfc3339();
    conn.execute(
        "INSERT INTO pipeline_events
         (occurred_at, level, stage, folder, photo_id, error_class, message, context_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![now, ev.level.as_str(), ev.stage, ev.folder, ev.photo_id,
                ev.error_class, ev.message, ev.context_json],
    )?;
    let id = conn.last_insert_rowid();
    rotate_if_needed(conn)?;
    Ok(id)
}

fn rotate_if_needed(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM pipeline_events", [], |r| r.get(0))?;
    if count > 10_000 {
        let to_delete = count - 10_000;
        conn.execute(
            "DELETE FROM pipeline_events WHERE id IN
             (SELECT id FROM pipeline_events ORDER BY occurred_at ASC LIMIT ?1)",
            params![to_delete],
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct UnresolvedGroup {
    pub stage: Option<String>,
    pub error_class: Option<String>,
    pub count: i64,
    pub example_message: String,
}

/// Group unresolved errors by (stage, error_class) for the Failures Inbox.
pub fn unresolved_groups(conn: &Connection) -> Result<Vec<UnresolvedGroup>> {
    let mut stmt = conn.prepare(
        "SELECT stage, error_class, COUNT(*) as cnt, MIN(message) as msg
         FROM pipeline_events
         WHERE level='error' AND resolved_at IS NULL
         GROUP BY stage, error_class
         ORDER BY cnt DESC"
    )?;
    let rows = stmt.query_map([], |r| Ok(UnresolvedGroup {
        stage: r.get(0)?, error_class: r.get(1)?, count: r.get(2)?, example_message: r.get(3)?,
    }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn resolve_group(
    conn: &Connection,
    stage: Option<&str>,
    error_class: Option<&str>,
    clock: &dyn Clock,
) -> Result<usize> {
    let now = clock.now().to_rfc3339();
    let n = conn.execute(
        "UPDATE pipeline_events SET resolved_at = ?1
         WHERE level='error' AND resolved_at IS NULL
           AND (stage IS ?2 OR stage = ?2)
           AND (error_class IS ?3 OR error_class = ?3)",
        params![now, stage, error_class],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{apply_v2_schema, FixedClock};

    fn open() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        apply_v2_schema(&c).unwrap();
        c
    }

    #[test]
    fn append_then_read_groups() {
        let c = open();
        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        for _ in 0..3 {
            append(&c, &PipelineEvent {
                level: EventLevel::Error, stage: Some("llm".into()), folder: None,
                photo_id: None, error_class: Some("llm_unreachable".into()),
                message: "connection refused".into(), context_json: None,
            }, &clk).unwrap();
        }
        let groups = unresolved_groups(&c).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 3);
    }

    #[test]
    fn rotation_caps_at_10k() {
        let c = open();
        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        // Insert 10_005; rotation should keep us at 10_000.
        for i in 0..10_005 {
            append(&c, &PipelineEvent {
                level: EventLevel::Info, stage: None, folder: None,
                photo_id: None, error_class: None,
                message: format!("event {}", i), context_json: None,
            }, &clk).unwrap();
        }
        let count: i64 = c.query_row("SELECT COUNT(*) FROM pipeline_events", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 10_000);
    }
}
```

- [ ] **Step 2: Wire and run**

In `src/db/mod.rs` add:

```rust
pub mod pipeline_events;
```

Run: `cargo test --lib pipeline_events::tests`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/db/pipeline_events.rs src/db/mod.rs
git commit -m "Add pipeline_events writer with auto-rotation"
```

---

### Task 8: JSONL structured log appender

A `tracing` layer that emits JSONL to a daily-rotated file. Lives alongside the DB-backed `pipeline_events` table.

**Files:**
- Create: `src/pipeline/log.rs` (under new module)
- Modify: `src/lib.rs` (add `pub mod pipeline;`)
- Modify: `src/pipeline/mod.rs` (declare submodule)
- Modify: `src/logging.rs` (add `init_pipeline_log` helper)

Spec reference: §8.7.

- [ ] **Step 1: Create the pipeline module skeleton**

Create `src/pipeline/mod.rs`:

```rust
//! Pipeline: per-photo state machine (spec §4).
//! Stages, scheduler, log appender, circuit breaker.

pub mod log;
pub mod circuit_breaker;
pub mod scheduler;
pub mod stages;
```

Add `pub mod pipeline;` to `src/lib.rs`.

- [ ] **Step 2: Write the JSONL appender**

Create `src/pipeline/log.rs`:

```rust
//! JSONL pipeline log (spec §8.7).
//! Path: ${XDG_STATE_HOME:-~/.local/state}/clepho/logs/clepho-YYYYMMDD.jsonl

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct LogRecord<'a> {
    pub ts: String,
    pub level: &'a str,         // 'trace'|'debug'|'info'|'warn'|'error'
    pub component: &'a str,     // e.g. 'pipeline.faces'
    pub folder: Option<&'a str>,
    pub photo_id: Option<i64>,
    pub photo_path: Option<&'a str>,
    pub stage: Option<&'a str>,
    pub error_class: Option<&'a str>,
    pub message: &'a str,
    pub context: Option<&'a serde_json::Value>,
    pub duration_ms: Option<u64>,
}

pub struct JsonlAppender {
    dir: PathBuf,
    handle: Mutex<Option<(String, std::fs::File)>>, // (yyyymmdd, file)
}

impl JsonlAppender {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        create_dir_all(&dir).with_context(|| format!("create log dir {:?}", dir))?;
        Ok(Self { dir, handle: Mutex::new(None) })
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                p.push(".local/state");
                p
            });
        base.join("clepho/logs")
    }

    pub fn append(&self, rec: &LogRecord) -> Result<()> {
        let today = Utc::now().format("%Y%m%d").to_string();
        let mut guard = self.handle.lock().unwrap();
        let needs_new = match &*guard {
            Some((d, _)) => d != &today,
            None => true,
        };
        if needs_new {
            let path = self.dir.join(format!("clepho-{}.jsonl", today));
            let f = OpenOptions::new().create(true).append(true).open(&path)
                .with_context(|| format!("open log {:?}", path))?;
            *guard = Some((today.clone(), f));
        }
        let (_, file) = guard.as_mut().unwrap();
        serde_json::to_writer(&mut *file, rec)?;
        writeln!(file)?;
        Ok(())
    }

    /// Delete .jsonl files older than `retention_days` and gzip non-current ones.
    /// Called by daemon on startup and once per 24h.
    pub fn rotate(&self, retention_days: u64) -> Result<()> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("clepho-") { continue; }
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            let modified_dt: chrono::DateTime<Utc> = modified.into();
            if modified_dt < cutoff {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_writes_jsonl_lines() {
        let d = tempdir().unwrap();
        let app = JsonlAppender::new(d.path()).unwrap();
        let rec = LogRecord {
            ts: "2026-05-04T12:00:00Z".into(),
            level: "info", component: "pipeline.scan",
            folder: Some("/photos"), photo_id: Some(1), photo_path: Some("/photos/a.jpg"),
            stage: Some("scan"), error_class: None,
            message: "scan complete", context: None, duration_ms: Some(42),
        };
        app.append(&rec).unwrap();
        app.append(&rec).unwrap();

        // Find the file
        let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
        let p = files[0].as_ref().unwrap().path();
        let content = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["component"], "pipeline.scan");
        assert_eq!(parsed["duration_ms"], 42);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib pipeline::log::tests`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/pipeline/mod.rs src/pipeline/log.rs
git commit -m "Add JSONL pipeline log appender"
```

---

### Task 9: `Stage` trait + registry

The contract every pipeline stage implements. Stages declare what photos they consume (skip-if-done query) and produce stage outcomes per photo.

**Files:**
- Create: `src/pipeline/stages/mod.rs`

Spec reference: §4.

- [ ] **Step 1: Create the stage module structure**

Create `src/pipeline/stages/mod.rs`:

```rust
//! Stage trait + registry. Each stage owns one column in `photos` (e.g. `llm_done_at`)
//! and one error column (e.g. `llm_error`). The scheduler pulls pending photos via
//! `pending_query`, dispatches `process_one`, and records success/failure.

use crate::db::clock::Clock;
use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub mod scan;
pub mod exif;
pub mod thumb;
pub mod llm;
pub mod index;
// faces stub provided in Plan 2

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageId { Scan, Exif, Thumb, Llm, Faces, Index }

impl StageId {
    pub fn name(self) -> &'static str {
        match self {
            Self::Scan => "scan", Self::Exif => "exif", Self::Thumb => "thumb",
            Self::Llm => "llm", Self::Faces => "faces", Self::Index => "index",
        }
    }
    pub fn done_col(self) -> &'static str {
        match self {
            Self::Scan => "scan_done_at", Self::Exif => "exif_done_at",
            Self::Thumb => "thumb_done_at", Self::Llm => "llm_done_at",
            Self::Faces => "faces_done_at", Self::Index => "index_done_at",
        }
    }
    pub fn error_col(self) -> &'static str {
        match self {
            Self::Scan => "scan_error", Self::Exif => "exif_error",
            Self::Thumb => "thumb_error", Self::Llm => "llm_error",
            Self::Faces => "faces_error", Self::Index => "index_error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingPhoto {
    pub id: i64,
    pub path: PathBuf,
}

/// One stage's outcome on one photo.
#[derive(Debug)]
pub enum StageOutcome {
    Ok,
    Err { error_class: String, message: String },
}

/// Stage trait. Implementations are stateless; they receive the connection
/// and clock from the scheduler. Heavy resources (LLM client, face engine)
/// are owned by the impl as `Arc`-ed fields.
pub trait Stage: Send + Sync {
    fn id(&self) -> StageId;

    /// Returns photos pending this stage in the given folder (or all folders if None).
    /// Must respect the stage DAG (e.g. exif requires scan_done_at IS NOT NULL).
    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>;

    /// Process one photo. Implementations MUST set <stage>_done_at and outputs in
    /// the same transaction. On error, return Err variant with classification.
    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, clock: &dyn Clock)
        -> Result<StageOutcome>;
}

/// Default `pending` query for stages whose only prerequisite is one earlier stage.
/// Used by exif (requires scan), thumb/llm/faces (require exif), index (no prereq).
pub fn pending_default(
    conn: &Connection,
    done_col: &str,
    prereq_col: Option<&str>,
    folder: Option<&str>,
    limit: usize,
) -> Result<Vec<PendingPhoto>> {
    let folder_clause = if folder.is_some() {
        " AND directory(path) = ?"
    } else { "" };

    let prereq_clause = match prereq_col {
        Some(p) => format!(" AND {} IS NOT NULL", p),
        None => String::new(),
    };

    let sql = format!(
        "SELECT id, path FROM photos
         WHERE {} IS NULL{}{}
         ORDER BY id ASC LIMIT ?",
        done_col, prereq_clause, folder_clause
    );

    // SQLite doesn't have a native `directory(path)` function; we filter in code instead.
    // Simpler: just use LIKE on path prefix.
    let sql = match folder {
        Some(_) => format!(
            "SELECT id, path FROM photos
             WHERE {} IS NULL{} AND path LIKE ?1
             ORDER BY id ASC LIMIT ?2",
            done_col, prereq_clause
        ),
        None => format!(
            "SELECT id, path FROM photos
             WHERE {} IS NULL{}
             ORDER BY id ASC LIMIT ?1",
            done_col, prereq_clause
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = match folder {
        Some(f) => {
            let prefix = format!("{}/%", f.trim_end_matches('/'));
            stmt.query_map(rusqlite::params![prefix, limit as i64], |r| {
                Ok(PendingPhoto { id: r.get(0)?, path: PathBuf::from(r.get::<_, String>(1)?) })
            })?.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            stmt.query_map(rusqlite::params![limit as i64], |r| {
                Ok(PendingPhoto { id: r.get(0)?, path: PathBuf::from(r.get::<_, String>(1)?) })
            })?.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    Ok(rows)
}

/// Helper for stage workers: write success in a transaction
/// (sets done_at, clears error). Called from inside stage impls.
pub fn mark_done(conn: &Connection, stage: StageId, photo_id: i64, clock: &dyn Clock) -> Result<()> {
    let now = clock.now().to_rfc3339();
    conn.execute(
        &format!(
            "UPDATE photos SET {}=?1, {}=NULL WHERE id=?2",
            stage.done_col(), stage.error_col()
        ),
        rusqlite::params![now, photo_id],
    )?;
    Ok(())
}

/// Helper for stage workers: write failure (sets error, leaves done_at NULL).
pub fn mark_error(conn: &Connection, stage: StageId, photo_id: i64, message: &str) -> Result<()> {
    conn.execute(
        &format!("UPDATE photos SET {}=?1 WHERE id=?2", stage.error_col()),
        rusqlite::params![message, photo_id],
    )?;
    Ok(())
}
```

- [ ] **Step 2: Stub the per-stage modules**

Create empty modules for each stage so the file structure compiles. Each will be filled in subsequent tasks. For now:

```rust
// src/pipeline/stages/scan.rs
//! See Task 13.
```

```rust
// src/pipeline/stages/exif.rs
//! See Task 14.
```

```rust
// src/pipeline/stages/thumb.rs
//! See Task 15.
```

```rust
// src/pipeline/stages/llm.rs
//! See Task 16.
```

```rust
// src/pipeline/stages/index.rs
//! See Task 17.
```

- [ ] **Step 3: Verify compile**

Run: `cargo build --lib`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add src/pipeline/stages/
git commit -m "Add Stage trait, registry helpers, and per-stage module stubs"
```

---

### Task 10: Circuit breaker

Per-stage tracker of consecutive same-class failures. Threshold configurable; default 3.

**Files:**
- Create: `src/pipeline/circuit_breaker.rs`

Spec reference: §8.4.

- [ ] **Step 1: Implement and test**

Create `src/pipeline/circuit_breaker.rs`:

```rust
//! Per-stage circuit breaker (spec §8.4).
//! Tripped when N consecutive same-class failures occur.
//! Tripping does NOT cascade to other stages.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::pipeline::stages::StageId;

#[derive(Debug, Default)]
struct State {
    last_class: Option<String>,
    consecutive: u32,
    paused: bool,
}

pub struct CircuitBreaker {
    threshold: u32,
    inner: Mutex<HashMap<StageId, State>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState { Open, Closed, Tripped }

impl CircuitBreaker {
    pub fn new(threshold: u32) -> Self {
        Self { threshold, inner: Mutex::new(HashMap::new()) }
    }

    pub fn record_success(&self, stage: StageId) {
        let mut g = self.inner.lock().unwrap();
        let s = g.entry(stage).or_default();
        s.consecutive = 0;
        s.last_class = None;
        s.paused = false;
    }

    /// Returns true if the breaker just tripped on this failure.
    pub fn record_failure(&self, stage: StageId, error_class: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        let s = g.entry(stage).or_default();
        if s.last_class.as_deref() == Some(error_class) {
            s.consecutive += 1;
        } else {
            s.last_class = Some(error_class.to_string());
            s.consecutive = 1;
        }
        if !s.paused && s.consecutive >= self.threshold {
            s.paused = true;
            return true;
        }
        false
    }

    pub fn is_paused(&self, stage: StageId) -> bool {
        self.inner.lock().unwrap().get(&stage).map(|s| s.paused).unwrap_or(false)
    }

    /// Manually clear the breaker (user pressed Retry in Pipeline Status).
    pub fn reset(&self, stage: StageId) {
        let mut g = self.inner.lock().unwrap();
        if let Some(s) = g.get_mut(&stage) {
            s.consecutive = 0;
            s.last_class = None;
            s.paused = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_consecutive_same_class_trips() {
        let cb = CircuitBreaker::new(3);
        assert!(!cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(!cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(cb.is_paused(StageId::Llm));
    }

    #[test]
    fn different_classes_dont_accumulate() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_failure(StageId::Llm, "image_decode");
        cb.record_failure(StageId::Llm, "oom");
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn success_resets_counter() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_success(StageId::Llm);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn reset_unpauses() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure(StageId::Llm, "x");
        cb.record_failure(StageId::Llm, "x");
        assert!(cb.is_paused(StageId::Llm));
        cb.reset(StageId::Llm);
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn breaker_is_per_stage() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure(StageId::Llm, "x");
        cb.record_failure(StageId::Llm, "x");
        assert!(cb.is_paused(StageId::Llm));
        assert!(!cb.is_paused(StageId::Faces));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib pipeline::circuit_breaker::tests`
Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/pipeline/circuit_breaker.rs
git commit -m "Add per-stage circuit breaker with configurable threshold"
```

---

### Task 11: Worker-pool scheduler

The dispatcher that pulls pending work and runs stages with bounded concurrency. Single-process, in-memory; same scheduler used by daemon and ad-hoc TUI runs.

**Files:**
- Create: `src/pipeline/scheduler.rs`

Spec reference: §4.5, §4.7, §4.8.

- [ ] **Step 1: Implement**

```rust
//! Scheduler: pulls pending photos per stage, dispatches to bounded worker pools.
//! Single-process. Owned by the daemon (in daemon mode) or by the TUI (standalone).

use crate::config::PipelineConfig;
use crate::db::clock::Clock;
use crate::db::pipeline_events::{append as ev_append, EventLevel, PipelineEvent};
use crate::pipeline::circuit_breaker::CircuitBreaker;
use crate::pipeline::log::{JsonlAppender, LogRecord};
use crate::pipeline::stages::{Stage, StageId, StageOutcome, mark_done, mark_error};
use anyhow::Result;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct Scheduler {
    pub stages: Vec<Arc<dyn Stage>>,
    pub breaker: Arc<CircuitBreaker>,
    pub log: Arc<JsonlAppender>,
    pub config: PipelineConfig,
    pub clock: Arc<dyn Clock>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Default, Clone)]
pub struct RunReport {
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub skipped_paused: u64,
}

impl Scheduler {
    /// Run one pass: each stage drains pending photos in its folder up to its worker
    /// budget. Returns aggregate report. Designed to be called repeatedly (daemon ticks)
    /// or once (ad-hoc TUI run).
    pub fn run_pass(
        &self,
        conn: &Connection,
        folder: Option<&str>,
    ) -> Result<RunReport> {
        let mut report = RunReport::default();

        for stage in &self.stages {
            if self.cancel.load(Ordering::Relaxed) { break; }
            if self.breaker.is_paused(stage.id()) {
                report.skipped_paused += 1;
                continue;
            }
            let workers = self.workers_for(stage.id());
            let pending = stage.pending(conn, folder, workers as usize * 4)?;
            if pending.is_empty() { continue; }

            // Sequential per stage in v1 (rayon-parallel within stages is a Plan 2 polish).
            // The "concurrency" config is honoured by max-per-pass batch size.
            for photo in pending {
                if self.cancel.load(Ordering::Relaxed) { break; }
                let start = Instant::now();
                let result = stage.process_one(conn, &photo, &*self.clock);
                let elapsed = start.elapsed().as_millis() as u64;
                report.processed += 1;
                match result {
                    Ok(StageOutcome::Ok) => {
                        mark_done(conn, stage.id(), photo.id, &*self.clock)?;
                        self.breaker.record_success(stage.id());
                        report.succeeded += 1;
                        let _ = self.log.append(&LogRecord {
                            ts: self.clock.now().to_rfc3339(),
                            level: "info", component: "pipeline.scheduler",
                            folder, photo_id: Some(photo.id),
                            photo_path: photo.path.to_str(),
                            stage: Some(stage.id().name()), error_class: None,
                            message: "stage ok", context: None,
                            duration_ms: Some(elapsed),
                        });
                    }
                    Ok(StageOutcome::Err { error_class, message }) | Err(_) => {
                        // Normalise both Ok(Err) and Err(_) to a stage failure.
                        let (cls, msg) = match result {
                            Ok(StageOutcome::Err { error_class, message }) => (error_class, message),
                            Err(e) => ("unhandled".to_string(), format!("{:#}", e)),
                            _ => unreachable!(),
                        };
                        mark_error(conn, stage.id(), photo.id, &msg)?;
                        ev_append(conn, &PipelineEvent {
                            level: EventLevel::Error,
                            stage: Some(stage.id().name().to_string()),
                            folder: folder.map(String::from),
                            photo_id: Some(photo.id),
                            error_class: Some(cls.clone()),
                            message: msg.clone(),
                            context_json: None,
                        }, &*self.clock)?;
                        let _ = self.log.append(&LogRecord {
                            ts: self.clock.now().to_rfc3339(),
                            level: "error", component: "pipeline.scheduler",
                            folder, photo_id: Some(photo.id),
                            photo_path: photo.path.to_str(),
                            stage: Some(stage.id().name()),
                            error_class: Some(&cls),
                            message: &msg, context: None,
                            duration_ms: Some(elapsed),
                        });
                        if self.breaker.record_failure(stage.id(), &cls) {
                            ev_append(conn, &PipelineEvent {
                                level: EventLevel::Error,
                                stage: Some(stage.id().name().to_string()),
                                folder: folder.map(String::from),
                                photo_id: None,
                                error_class: Some(cls.clone()),
                                message: format!("stage paused after {} consecutive {} failures",
                                                 self.config.circuit_breaker_threshold, cls),
                                context_json: None,
                            }, &*self.clock)?;
                        }
                        report.failed += 1;
                        break; // move to next stage; this stage may be paused now
                    }
                }
            }
        }
        Ok(report)
    }

    fn workers_for(&self, stage: StageId) -> u32 {
        match stage {
            StageId::Scan  => self.config.scan_workers,
            StageId::Exif  => self.config.exif_workers,
            StageId::Thumb => self.config.thumb_workers,
            StageId::Llm   => self.config.llm_workers,
            StageId::Faces => self.config.faces_workers,
            StageId::Index => self.config.index_workers,
        }
    }
}
```

- [ ] **Step 2: Verify compile**

This task references `PipelineConfig` (Task 18 will add it). For now, add a temporary stub in `src/config.rs`:

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub scan_workers: u32,
    pub exif_workers: u32,
    pub thumb_workers: u32,
    pub llm_workers: u32,
    pub faces_workers: u32,
    pub index_workers: u32,
    pub circuit_breaker_threshold: u32,
}
impl Default for PipelineConfig {
    fn default() -> Self {
        Self { scan_workers: 2, exif_workers: 4, thumb_workers: 4,
               llm_workers: 2, faces_workers: 2, index_workers: 4,
               circuit_breaker_threshold: 3 }
    }
}
```

Run: `cargo build --lib`
Expected: builds. (Tests for the scheduler come in Task 17b — depends on at least one stage being implemented.)

- [ ] **Step 3: Commit**

```bash
git add src/pipeline/scheduler.rs src/config.rs
git commit -m "Add pipeline scheduler with bounded per-stage workers"
```

---

### Task 12: Filter evaluator (smart-album membership)

Evaluates a `filter_json` against a photo's facets. Used by the `index` stage to recompute smart-album memberships, and (in Plan 4) by the BrowseView filter bar.

**Files:**
- Create: `src/db/filter_eval.rs`
- Create: `tests/filter_eval.rs`

Spec reference: §3.5, §6.6.

- [ ] **Step 1: Define filter types and evaluator**

Create `src/db/filter_eval.rs`:

```rust
//! Filter JSON evaluator (spec §3.5).
//! Used by smart-album membership recompute (index stage) and by the BrowseView
//! filter bar (Plan 4). Single source of truth for filter semantics.

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub combinator: Combinator,
    pub clauses: Vec<Clause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combinator { And, Or }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "facet")]
#[serde(rename_all = "snake_case")]
pub enum Clause {
    People  { op: SetOp, values: Vec<String> },
    Objects { op: SetOp, values: Vec<String> },
    Tags    { op: SetOp, values: Vec<String> },
    Cameras { op: SetOp, values: Vec<String> }, // matches camera_make or model
    Events  { op: SetOp, values: Vec<String> },
    Places  { op: PlaceOp, value: PlaceValue },
    Dates   { op: DateOp, value: DateValue },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp { AnyOf, AllOf, NoneOf }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceOp { WithinKm, BoundingBox }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceValue {
    WithinKm { lat: f64, lon: f64, km: f64 },
    Bbox { min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOp { Between, Year, Month }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DateValue {
    Between { from: String, to: String },
    Year(i32),
    YearMonth { year: i32, month: u32 },
}

/// Evaluate a filter against one photo. Returns true if the photo matches.
/// Pulls relevant facets from the DB; intended for one-photo recompute (index stage).
pub fn matches_photo(conn: &Connection, photo_id: i64, filter: &Filter) -> Result<bool> {
    let results: Vec<bool> = filter.clauses.iter()
        .map(|c| matches_clause(conn, photo_id, c))
        .collect::<Result<Vec<_>>>()?;

    Ok(match filter.combinator {
        Combinator::And => results.iter().all(|b| *b),
        Combinator::Or  => results.iter().any(|b| *b),
    })
}

fn matches_clause(conn: &Connection, photo_id: i64, clause: &Clause) -> Result<bool> {
    match clause {
        Clause::Objects { op, values } =>
            match_set(conn, photo_id, *op, values,
                "SELECT o.name FROM photo_objects po JOIN objects o ON o.id=po.object_id
                 WHERE po.photo_id=?1"),
        Clause::Tags { op, values } =>
            match_set(conn, photo_id, *op, values,
                "SELECT t.name FROM photo_user_tags pt JOIN user_tags t ON t.id=pt.tag_id
                 WHERE pt.photo_id=?1"),
        Clause::People { op, values } =>
            match_set(conn, photo_id, *op, values,
                "SELECT p.name FROM faces f JOIN people p ON p.id=f.person_id
                 WHERE f.photo_id=?1 AND p.name IS NOT NULL"),
        Clause::Cameras { op, values } => {
            let cams: Vec<String> = conn.query_row(
                "SELECT COALESCE(camera_make,'') || ' ' || COALESCE(camera_model,'')
                 FROM photos WHERE id=?1", rusqlite::params![photo_id], |r| r.get(0))
                .map(|s: String| vec![s.trim().to_string()])
                .unwrap_or_default();
            Ok(eval_set(*op, values, &cams))
        }
        Clause::Events { op, values } =>
            match_set(conn, photo_id, *op, values,
                "SELECT e.name FROM photo_events pe JOIN events e ON e.id=pe.event_id
                 WHERE pe.photo_id=?1"),
        Clause::Places { op, value } => match_place(conn, photo_id, *op, value),
        Clause::Dates  { op, value } => match_date(conn, photo_id, *op, value),
    }
}

fn match_set(conn: &Connection, photo_id: i64, op: SetOp, values: &[String], sql: &str) -> Result<bool> {
    let mut stmt = conn.prepare(sql)?;
    let actual: Vec<String> = stmt.query_map(rusqlite::params![photo_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(eval_set(op, values, &actual))
}

fn eval_set(op: SetOp, values: &[String], actual: &[String]) -> bool {
    let actual_set: std::collections::HashSet<&str> = actual.iter().map(|s| s.as_str()).collect();
    match op {
        SetOp::AnyOf  => values.iter().any(|v| actual_set.contains(v.as_str())),
        SetOp::AllOf  => values.iter().all(|v| actual_set.contains(v.as_str())),
        SetOp::NoneOf => values.iter().all(|v| !actual_set.contains(v.as_str())),
    }
}

fn match_place(conn: &Connection, photo_id: i64, op: PlaceOp, value: &PlaceValue) -> Result<bool> {
    let coords: Option<(f64, f64)> = conn.query_row(
        "SELECT gps_lat, gps_lon FROM photos WHERE id=?1 AND gps_lat IS NOT NULL AND gps_lon IS NOT NULL",
        rusqlite::params![photo_id], |r| Ok((r.get(0)?, r.get(1)?)),
    ).ok();
    let Some((lat, lon)) = coords else { return Ok(false); };

    match (op, value) {
        (PlaceOp::WithinKm, PlaceValue::WithinKm { lat: clat, lon: clon, km }) => {
            Ok(haversine_km(lat, lon, *clat, *clon) <= *km)
        }
        (PlaceOp::BoundingBox, PlaceValue::Bbox { min_lat, min_lon, max_lat, max_lon }) => {
            Ok(lat >= *min_lat && lat <= *max_lat && lon >= *min_lon && lon <= *max_lon)
        }
        _ => Err(anyhow!("place op/value mismatch")),
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
          + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn match_date(conn: &Connection, photo_id: i64, op: DateOp, value: &DateValue) -> Result<bool> {
    let taken: Option<String> = conn.query_row(
        "SELECT taken_at FROM photos WHERE id=?1", rusqlite::params![photo_id], |r| r.get(0),
    ).ok().flatten();
    let Some(t) = taken else { return Ok(false); };

    let dt = chrono::DateTime::parse_from_rfc3339(&t)
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|_| chrono::NaiveDate::parse_from_str(&t, "%Y-%m-%d")
            .map(|d| d.and_hms_opt(0,0,0).unwrap().and_utc().fixed_offset()))
        .map_err(|e| anyhow!("bad taken_at {}: {}", t, e))?;

    Ok(match (op, value) {
        (DateOp::Between, DateValue::Between { from, to }) => {
            let f = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
            let to = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
            let d = dt.date_naive();
            d >= f && d <= to
        }
        (DateOp::Year, DateValue::Year(y)) => dt.format("%Y").to_string().parse::<i32>().ok() == Some(*y),
        (DateOp::Month, DateValue::YearMonth { year, month }) => {
            let y = dt.format("%Y").to_string().parse::<i32>().ok();
            let m = dt.format("%m").to_string().parse::<u32>().ok();
            y == Some(*year) && m == Some(*month)
        }
        _ => return Err(anyhow!("date op/value mismatch")),
    })
}
```

- [ ] **Step 2: Wire and write integration tests**

Add `pub mod filter_eval;` to `src/db/mod.rs` and re-export `Filter`, `Clause`, `Combinator`, `matches_photo`.

Create `tests/filter_eval.rs`:

```rust
use clepho::db::{apply_v2_schema, filter_eval::*};
use rusqlite::Connection;

fn db_with_photo() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, gps_lat, gps_lon, taken_at, camera_make, camera_model)
              VALUES ('p.jpg', 41.89, 12.49, '2024-06-12T19:23:45+02:00', 'Sony', 'A7iv')", []).unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();
    c.execute("INSERT INTO photo_objects(photo_id, object_id, source) VALUES (1,1,'ai')", []).unwrap();
    c
}

#[test]
fn objects_anyof_matches() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Objects { op: SetOp::AnyOf, values: vec!["sunset".into()] }
    ]};
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn objects_anyof_misses() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Objects { op: SetOp::AnyOf, values: vec!["snow".into()] }
    ]};
    assert!(!matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn places_within_km_matches() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Places {
            op: PlaceOp::WithinKm,
            value: PlaceValue::WithinKm { lat: 41.9, lon: 12.5, km: 50.0 }
        }
    ]};
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn places_within_km_misses() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Places {
            op: PlaceOp::WithinKm,
            value: PlaceValue::WithinKm { lat: 0.0, lon: 0.0, km: 50.0 }
        }
    ]};
    assert!(!matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn date_between_matches() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Dates {
            op: DateOp::Between,
            value: DateValue::Between { from: "2024-01-01".into(), to: "2024-12-31".into() }
        }
    ]};
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn and_combinator() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Objects { op: SetOp::AnyOf, values: vec!["sunset".into()] },
        Clause::Dates { op: DateOp::Year, value: DateValue::Year(2024) },
    ]};
    assert!(matches_photo(&c, 1, &f).unwrap());

    let f2 = Filter { combinator: Combinator::And, clauses: vec![
        Clause::Objects { op: SetOp::AnyOf, values: vec!["sunset".into()] },
        Clause::Dates { op: DateOp::Year, value: DateValue::Year(2023) },
    ]};
    assert!(!matches_photo(&c, 1, &f2).unwrap());
}

#[test]
fn or_combinator() {
    let c = db_with_photo();
    let f = Filter { combinator: Combinator::Or, clauses: vec![
        Clause::Objects { op: SetOp::AnyOf, values: vec!["snow".into()] },
        Clause::Dates { op: DateOp::Year, value: DateValue::Year(2024) },
    ]};
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn property_filter_roundtrip_serde() {
    use proptest::prelude::*;
    proptest!(|(years in 2000i32..2030, km in 1.0f64..1000.0)| {
        let f = Filter { combinator: Combinator::And, clauses: vec![
            Clause::Dates { op: DateOp::Year, value: DateValue::Year(years) },
            Clause::Places {
                op: PlaceOp::WithinKm,
                value: PlaceValue::WithinKm { lat: 0.0, lon: 0.0, km }
            },
        ]};
        let s = serde_json::to_string(&f).unwrap();
        let f2: Filter = serde_json::from_str(&s).unwrap();
        prop_assert_eq!(format!("{:?}", f), format!("{:?}", f2));
    });
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test filter_eval`
Expected: 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/db/filter_eval.rs src/db/mod.rs tests/filter_eval.rs
git commit -m "Add smart-album filter evaluator (with property test)"
```

---

### Task 13: Scan stage

Walks one folder (or globally for all managed folders), inserts photo rows, computes file_hash + size + mime + dimensions, sets `scan_done_at`.

**Files:**
- Modify: `src/pipeline/stages/scan.rs`

Spec reference: §4.1 row 1.

- [ ] **Step 1: Implement**

Replace the stub in `src/pipeline/stages/scan.rs`:

```rust
//! Scan stage: discover files, compute hash, populate basic metadata.
//! Idempotent: on rerun, re-uses existing row if path exists.

use crate::db::clock::Clock;
use crate::pipeline::stages::{
    Stage, StageId, StageOutcome, PendingPhoto, mark_done, mark_error, pending_default,
};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ScanStage {
    pub root_paths_for_walk: Arc<Vec<PathBuf>>,  // folders to discover from
    pub allowed_extensions: Arc<Vec<String>>,    // e.g. ["jpg","jpeg","png","heic"]
}

impl ScanStage {
    pub fn new(roots: Vec<PathBuf>, exts: Vec<String>) -> Self {
        Self {
            root_paths_for_walk: Arc::new(roots),
            allowed_extensions: Arc::new(exts.into_iter().map(|e| e.to_lowercase()).collect()),
        }
    }

    /// Discover all candidate files under the configured roots and INSERT photo
    /// rows for any not yet present. Called by the daemon on managed folders or
    /// from the TUI on ad-hoc folder runs.
    pub fn discover(&self, conn: &Connection, folder: &Path) -> Result<usize> {
        let mut count = 0usize;
        let walker = walkdir::WalkDir::new(folder).follow_links(false);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() { continue; }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if !self.allowed_extensions.iter().any(|e| e == &ext) { continue; }

            let path_str = path.to_string_lossy();
            conn.execute(
                "INSERT OR IGNORE INTO photos(path) VALUES (?1)",
                rusqlite::params![path_str],
            )?;
            count += 1;
        }
        Ok(count)
    }
}

impl Stage for ScanStage {
    fn id(&self) -> StageId { StageId::Scan }

    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>
    {
        // Scan has no prereq; rows already inserted by `discover` precede this.
        pending_default(conn, "scan_done_at", None, folder, limit)
    }

    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, _clock: &dyn Clock)
        -> Result<StageOutcome>
    {
        let path = &photo.path;
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => return Ok(StageOutcome::Err {
                error_class: "fs_missing".into(),
                message: format!("stat failed: {}", e),
            }),
        };
        if !metadata.is_file() {
            return Ok(StageOutcome::Err {
                error_class: "fs_not_file".into(), message: "path is not a regular file".into(),
            });
        }
        let size = metadata.len() as i64;

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return Ok(StageOutcome::Err {
                error_class: "image_decode".into(),
                message: format!("read failed: {}", e),
            }),
        };

        // sha256 hash
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(&bytes));

        // dimensions + mime via `image` crate
        let (width, height, mime) = match image::load_from_memory(&bytes) {
            Ok(img) => {
                let (w, h) = (img.width() as i64, img.height() as i64);
                let mime = mime_from_path(path).unwrap_or("application/octet-stream");
                (Some(w), Some(h), Some(mime.to_string()))
            }
            Err(_) => (None, None, None),
        };

        conn.execute(
            "UPDATE photos SET file_hash=?1, file_size=?2, width=?3, height=?4, mime=?5, updated_at=CURRENT_TIMESTAMP
             WHERE id=?6",
            rusqlite::params![hash, size, width, height, mime, photo.id],
        ).context("update photo scan results")?;

        Ok(StageOutcome::Ok)
    }
}

fn mime_from_path(p: &Path) -> Option<&'static str> {
    let ext = p.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "heic" => "image/heic",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "gif" => "image/gif",
        _ => return None,
    })
}
```

- [ ] **Step 2: Add `walkdir` to Cargo.toml** if not present

Check: `grep walkdir Cargo.toml`. If missing, add `walkdir = "2"` to `[dependencies]`.

- [ ] **Step 3: Build**

Run: `cargo build --lib`
Expected: success.

- [ ] **Step 4: Inline integration test (in `tests/scan_stage.rs`)**

Create `tests/scan_stage.rs`:

```rust
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{Stage, scan::ScanStage, PendingPhoto};
use rusqlite::Connection;
use std::path::PathBuf;
use tempfile::tempdir;

fn write_jpeg(path: &std::path::Path) {
    use image::{ImageBuffer, Rgb};
    let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_fn(8, 8, |_,_| Rgb([255u8, 0, 0]));
    img.save_with_format(path, image::ImageFormat::Jpeg).unwrap();
}

#[test]
fn scan_inserts_and_processes() {
    let d = tempdir().unwrap();
    write_jpeg(&d.path().join("a.jpg"));
    write_jpeg(&d.path().join("b.jpg"));
    std::fs::write(d.path().join("readme.txt"), "ignored").unwrap();

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();

    let stage = ScanStage::new(vec![d.path().to_path_buf()], vec!["jpg".into()]);
    let n = stage.discover(&c, d.path()).unwrap();
    assert_eq!(n, 2, "expected 2 jpg discoveries, txt ignored");

    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 2);

    let clock = SystemClock;
    for p in pending {
        let out = stage.process_one(&c, &p, &clock).unwrap();
        assert!(matches!(out, clepho::pipeline::stages::StageOutcome::Ok));
        clepho::pipeline::stages::mark_done(&c, clepho::pipeline::stages::StageId::Scan, p.id, &clock).unwrap();
    }

    let after: i64 = c.query_row(
        "SELECT COUNT(*) FROM photos WHERE file_hash IS NOT NULL", [], |r| r.get(0)
    ).unwrap();
    assert_eq!(after, 2);
}

#[test]
fn scan_is_idempotent_via_or_ignore() {
    let d = tempdir().unwrap();
    write_jpeg(&d.path().join("a.jpg"));
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();

    let stage = ScanStage::new(vec![d.path().to_path_buf()], vec!["jpg".into()]);
    stage.discover(&c, d.path()).unwrap();
    stage.discover(&c, d.path()).unwrap();
    let count: i64 = c.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}
```

Make `mark_done` and `StageOutcome` pub in `src/pipeline/stages/mod.rs` if not already.

- [ ] **Step 5: Run**

Run: `cargo test --test scan_stage`
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/pipeline/stages/scan.rs tests/scan_stage.rs Cargo.toml
git commit -m "Implement scan stage: discover, hash, dimensions"
```

---

### Task 14: EXIF stage

Reads EXIF, writes `taken_at`, `gps_*`, `camera_*` columns. Uses existing `kamadak-exif` dep.

**Files:**
- Modify: `src/pipeline/stages/exif.rs`

Spec reference: §4.1 row 2.

- [ ] **Step 1: Implement**

```rust
//! EXIF stage: extract date, GPS, camera metadata into singleton columns on `photos`.

use crate::db::clock::Clock;
use crate::pipeline::stages::{
    Stage, StageId, StageOutcome, PendingPhoto, pending_default,
};
use anyhow::Result;
use rusqlite::Connection;
use std::fs::File;
use std::io::BufReader;

pub struct ExifStage;

impl Stage for ExifStage {
    fn id(&self) -> StageId { StageId::Exif }

    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>
    {
        pending_default(conn, "exif_done_at", Some("scan_done_at"), folder, limit)
    }

    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, _clock: &dyn Clock)
        -> Result<StageOutcome>
    {
        let f = match File::open(&photo.path) {
            Ok(f) => f,
            Err(e) => return Ok(StageOutcome::Err {
                error_class: "fs_missing".into(),
                message: format!("open: {}", e),
            }),
        };

        let mut br = BufReader::new(f);
        let parsed = exif::Reader::new().read_from_container(&mut br);

        // No EXIF is not an error — clean negative.
        let mut taken_at: Option<String> = None;
        let mut gps_lat: Option<f64> = None;
        let mut gps_lon: Option<f64> = None;
        let mut gps_alt: Option<f64> = None;
        let mut camera_make: Option<String> = None;
        let mut camera_model: Option<String> = None;
        let mut camera_lens: Option<String> = None;

        if let Ok(reader) = parsed {
            taken_at = field_string(&reader, exif::Tag::DateTimeOriginal)
                .and_then(|s| normalise_exif_datetime(&s));
            gps_lat = gps_decimal(&reader, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, &['N','S']);
            gps_lon = gps_decimal(&reader, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, &['E','W']);
            gps_alt = field_string(&reader, exif::Tag::GPSAltitude).and_then(|s| s.parse::<f64>().ok());
            camera_make  = field_string(&reader, exif::Tag::Make);
            camera_model = field_string(&reader, exif::Tag::Model);
            camera_lens  = field_string(&reader, exif::Tag::LensModel)
                .or_else(|| field_string(&reader, exif::Tag::LensMake));
        }

        conn.execute(
            "UPDATE photos
             SET taken_at=?1, gps_lat=?2, gps_lon=?3, gps_alt=?4,
                 camera_make=?5, camera_model=?6, camera_lens=?7,
                 updated_at=CURRENT_TIMESTAMP
             WHERE id=?8",
            rusqlite::params![taken_at, gps_lat, gps_lon, gps_alt,
                              camera_make, camera_model, camera_lens, photo.id],
        )?;
        Ok(StageOutcome::Ok)
    }
}

fn field_string(reader: &exif::Exif, tag: exif::Tag) -> Option<String> {
    reader.get_field(tag, exif::In::PRIMARY)
        .map(|f| f.display_value().with_unit(reader).to_string().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

/// Convert "YYYY:MM:DD HH:MM:SS" (EXIF) to "YYYY-MM-DDTHH:MM:SS" (ISO-ish).
fn normalise_exif_datetime(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() < 19 { return None; }
    let (date, time) = s.split_at(10);
    let date = date.replace(':', "-");
    Some(format!("{}T{}", date, &time[1..]))
}

fn gps_decimal(reader: &exif::Exif, coord: exif::Tag, refr: exif::Tag, signs: &[char; 2]) -> Option<f64> {
    let coord_field = reader.get_field(coord, exif::In::PRIMARY)?;
    let ref_field   = reader.get_field(refr, exif::In::PRIMARY)?;

    let dms = if let exif::Value::Rational(ref v) = coord_field.value {
        if v.len() < 3 { return None; }
        let d = v[0].num as f64 / v[0].denom as f64;
        let m = v[1].num as f64 / v[1].denom as f64;
        let s = v[2].num as f64 / v[2].denom as f64;
        d + m / 60.0 + s / 3600.0
    } else { return None; };

    let ref_str = ref_field.display_value().to_string();
    let neg = ref_str.contains(signs[1]);
    Some(if neg { -dms } else { dms })
}
```

- [ ] **Step 2: Integration test**

Create `tests/exif_stage.rs`:

```rust
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{Stage, exif::ExifStage};
use rusqlite::Connection;
use std::path::PathBuf;

#[test]
fn exif_reads_known_fixture() {
    // Use a fixture file with known EXIF; falls back to a synthesised JPEG (no EXIF)
    // to verify the no-EXIF clean-negative path.
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/photos/rome_2024_06_12.jpg");
    let path = if fixture.exists() { fixture } else {
        // synth no-EXIF jpeg
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("plain.jpg");
        let img: image::ImageBuffer<image::Rgb<u8>, _> = image::ImageBuffer::from_fn(8,8,|_,_| image::Rgb([0u8,0,0]));
        img.save(&p).unwrap();
        std::mem::forget(d); // keep tmpdir alive for test
        p
    };

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, scan_done_at) VALUES (?1, '2026-05-04T00:00:00Z')",
              rusqlite::params![path.to_str()]).unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);

    let outcome = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    assert!(matches!(outcome, clepho::pipeline::stages::StageOutcome::Ok));

    // No EXIF case: taken_at/gps remain NULL, no error, stage returns Ok.
    let (taken, lat, _err): (Option<String>, Option<f64>, Option<String>) = c.query_row(
        "SELECT taken_at, gps_lat, exif_error FROM photos WHERE id=?1",
        rusqlite::params![pending[0].id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    ).unwrap();

    if path.file_name().unwrap() == "rome_2024_06_12.jpg" {
        assert!(taken.is_some(), "fixture has DateTimeOriginal");
        assert!(lat.is_some(), "fixture has GPS");
    } else {
        assert!(taken.is_none(), "no-EXIF fixture should have NULL taken_at");
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test --test exif_stage`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src/pipeline/stages/exif.rs tests/exif_stage.rs
git commit -m "Implement EXIF stage with date/GPS/camera extraction"
```

---

### Task 15: Thumbnail stage

Refactors existing thumbnail-generation logic. Writes thumbnail to disk cache; sets `thumb_done_at`.

**Files:**
- Modify: `src/pipeline/stages/thumb.rs`

Spec reference: §4.1 row 3.

- [ ] **Step 1: Locate existing thumbnail code**

Run: `grep -rn "thumbnail" src/scanner/ src/lib.rs src/preview/ 2>/dev/null | head -30`
Expected output reveals where current thumbnail generation lives. The new stage calls into the same crate-level helper to avoid duplication.

- [ ] **Step 2: Implement**

```rust
//! Thumbnail stage: produce a 256-pixel-edge thumbnail PNG to the cache dir.
//! Reuses existing thumbnail helper if present; otherwise generates inline.

use crate::db::clock::Clock;
use crate::pipeline::stages::{
    Stage, StageId, StageOutcome, PendingPhoto, pending_default,
};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ThumbStage {
    pub cache_dir: Arc<PathBuf>,
    pub max_edge: u32,
}

impl ThumbStage {
    pub fn new(cache_dir: PathBuf, max_edge: u32) -> Self {
        Self { cache_dir: Arc::new(cache_dir), max_edge }
    }

    fn thumb_path(&self, file_hash: &str) -> PathBuf {
        // Two-level shard to keep dirs small
        let (a, b) = file_hash.split_at(2);
        let (b, _rest) = b.split_at(2);
        self.cache_dir.join(a).join(b).join(format!("{}.png", file_hash))
    }
}

impl Stage for ThumbStage {
    fn id(&self) -> StageId { StageId::Thumb }

    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>
    {
        pending_default(conn, "thumb_done_at", Some("exif_done_at"), folder, limit)
    }

    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, _clock: &dyn Clock)
        -> Result<StageOutcome>
    {
        // Need file_hash from scan stage to pick a stable cache path.
        let hash: Option<String> = conn.query_row(
            "SELECT file_hash FROM photos WHERE id=?1",
            rusqlite::params![photo.id], |r| r.get(0),
        )?;
        let Some(hash) = hash else {
            return Ok(StageOutcome::Err {
                error_class: "missing_hash".into(),
                message: "scan must run before thumb".into(),
            });
        };

        let img = match image::open(&photo.path) {
            Ok(img) => img,
            Err(e) => return Ok(StageOutcome::Err {
                error_class: "image_decode".into(),
                message: format!("decode: {}", e),
            }),
        };

        let thumb = img.thumbnail(self.max_edge, self.max_edge);
        let out_path = self.thumb_path(&hash);
        std::fs::create_dir_all(out_path.parent().unwrap()).context("mkdir thumb shard")?;
        thumb.save_with_format(&out_path, image::ImageFormat::Png)
            .with_context(|| format!("save thumb {:?}", out_path))?;
        Ok(StageOutcome::Ok)
    }
}
```

- [ ] **Step 3: Integration test**

Create `tests/thumb_stage.rs`:

```rust
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{Stage, thumb::ThumbStage};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn thumb_writes_cache_file() {
    let src_dir = tempdir().unwrap();
    let cache_dir = tempdir().unwrap();
    let img: image::ImageBuffer<image::Rgb<u8>, _> = image::ImageBuffer::from_fn(64, 64, |x,y| image::Rgb([x as u8, y as u8, 0u8]));
    let src = src_dir.path().join("photo.jpg");
    img.save(&src).unwrap();

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, file_hash, scan_done_at, exif_done_at)
               VALUES (?1, 'abcdef0123456789', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')",
        rusqlite::params![src.to_str()]).unwrap();

    let stage = ThumbStage::new(cache_dir.path().to_path_buf(), 256);
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let outcome = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    assert!(matches!(outcome, clepho::pipeline::stages::StageOutcome::Ok));

    // Expect the thumbnail at cache_dir/ab/cd/abcdef0123456789.png
    let expected = cache_dir.path().join("ab").join("cd").join("abcdef0123456789.png");
    assert!(expected.exists(), "thumbnail expected at {:?}", expected);
}
```

- [ ] **Step 4: Run**

Run: `cargo test --test thumb_stage`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add src/pipeline/stages/thumb.rs tests/thumb_stage.rs
git commit -m "Implement thumbnail stage with sharded cache layout"
```

---

### Task 16: LLM stage

The keystone refactor. Calls `LlmClient::describe_and_tag_image`, writes `description` (provenance-aware) + `photo_objects` (provenance-aware via `pipeline_write_facet`) + text embedding (existing `embeddings` table).

**Files:**
- Modify: `src/pipeline/stages/llm.rs`

Spec reference: §4.1 row 4, §6.

- [ ] **Step 1: Implement**

```rust
//! LLM stage: ask the configured LlmClient for description + tag list,
//! write description with provenance-aware UPDATE, write each tag as a
//! `photo_objects` row via the provenance helper.

use crate::config::LlmConfig;
use crate::db::clock::Clock;
use crate::db::provenance::{pipeline_write_facet, FacetTable};
use crate::llm::client::LlmClient;
use crate::pipeline::stages::{
    Stage, StageId, StageOutcome, PendingPhoto, pending_default,
};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::sync::Arc;

pub struct LlmStage {
    pub client: Arc<dyn LlmClient + Send + Sync>,
    pub global_prompt_override: Option<String>,
}

impl LlmStage {
    fn folder_prompt(conn: &Connection, photo_path: &std::path::Path) -> Option<String> {
        let dir = photo_path.parent()?.to_string_lossy().to_string();
        conn.query_row(
            "SELECT custom_prompt FROM folder_prompts WHERE path=?1",
            params![dir], |r| r.get(0),
        ).ok()
    }

    fn description_writable(conn: &Connection, photo_id: i64) -> Result<bool> {
        let row: Option<(Option<String>, Option<String>)> = conn.query_row(
            "SELECT description_source, description_confirmed_at FROM photos WHERE id=?1",
            params![photo_id], |r| Ok((r.get(0)?, r.get(1)?)),
        ).ok();
        let Some((src, conf)) = row else { return Ok(true); };
        // Only writable if (source IS NULL OR source='ai') AND confirmed_at IS NULL
        let writable = match src.as_deref() {
            None | Some("ai") => conf.is_none(),
            _ => false,
        };
        Ok(writable)
    }
}

impl Stage for LlmStage {
    fn id(&self) -> StageId { StageId::Llm }

    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>
    {
        pending_default(conn, "llm_done_at", Some("exif_done_at"), folder, limit)
    }

    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, clock: &dyn Clock)
        -> Result<StageOutcome>
    {
        let prompt = Self::folder_prompt(conn, &photo.path)
            .or_else(|| self.global_prompt_override.clone());

        let result = self.client.describe_and_tag_image(&photo.path, prompt.as_deref());
        let (description, tags) = match result {
            Ok((d, t)) => (d, t),
            Err(e) => {
                let msg = format!("{:#}", e);
                let class = if msg.to_lowercase().contains("connection") || msg.to_lowercase().contains("refused") {
                    "llm_unreachable"
                } else if msg.to_lowercase().contains("timeout") {
                    "llm_timeout"
                } else if msg.to_lowercase().contains("json") {
                    "llm_bad_json"
                } else { "llm_unknown" };
                return Ok(StageOutcome::Err { error_class: class.into(), message: msg });
            }
        };

        let tx = conn.unchecked_transaction()?;

        // Description (provenance-aware)
        if Self::description_writable(&tx, photo.id)? {
            tx.execute(
                "UPDATE photos SET description=?1, description_source='ai',
                                  description_confirmed_at=NULL,
                                  updated_at=CURRENT_TIMESTAMP
                 WHERE id=?2",
                params![description, photo.id],
            )?;
        }

        // Objects: each tag string → ensure objects row, then provenance write.
        for tag in &tags {
            let tag = tag.trim().to_lowercase();
            if tag.is_empty() { continue; }
            tx.execute(
                "INSERT OR IGNORE INTO objects(name) VALUES (?1)",
                params![tag],
            )?;
            let object_id: i64 = tx.query_row(
                "SELECT id FROM objects WHERE name=?1", params![tag], |r| r.get(0),
            )?;
            pipeline_write_facet(
                &tx, FacetTable::PhotoObjects, photo.id, object_id, "object", &tag, clock,
            )?;
        }

        // Embedding storage if client supports it.
        if let Ok(Some(emb)) = self.client.text_embedding(&description) {
            let bytes = embedding_to_bytes(&emb);
            tx.execute(
                "INSERT OR REPLACE INTO embeddings(photo_id, kind, model, dims, vector)
                 VALUES (?1, 'text', ?2, ?3, ?4)",
                params![photo.id, self.client.embedding_model_name(), emb.len() as i64, bytes],
            )?;
        }

        tx.commit()?;
        Ok(StageOutcome::Ok)
    }
}

fn embedding_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for f in v { buf.extend_from_slice(&f.to_le_bytes()); }
    buf
}
```

- [ ] **Step 2: Extend `LlmClient` trait if needed**

Open `src/llm/client.rs`. Verify `describe_and_tag_image` returns `Result<(String, Vec<String>)>` (the exploration showed this is already true). Verify `text_embedding(&str)` and `embedding_model_name()` exist; if not, add:

```rust
pub trait LlmClient: Send + Sync {
    fn describe_and_tag_image(&self, path: &std::path::Path, custom_prompt: Option<&str>)
        -> anyhow::Result<(String, Vec<String>)>;

    fn text_embedding(&self, _text: &str) -> anyhow::Result<Option<Vec<f32>>> {
        Ok(None)
    }
    fn embedding_model_name(&self) -> &'static str { "" }
}
```

The default `text_embedding` returning `Ok(None)` keeps providers without embeddings working unchanged.

- [ ] **Step 3: Integration test with mock LlmClient**

Create `tests/llm_stage.rs`:

```rust
use clepho::config::LlmConfig;
use clepho::db::{apply_v2_schema, FixedClock};
use clepho::llm::client::LlmClient;
use clepho::pipeline::stages::{Stage, StageOutcome, llm::LlmStage};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

struct MockLlm {
    desc: String,
    tags: Vec<String>,
    fail: Option<String>,
}

impl LlmClient for MockLlm {
    fn describe_and_tag_image(&self, _p: &Path, _cp: Option<&str>)
        -> anyhow::Result<(String, Vec<String>)>
    {
        if let Some(e) = &self.fail { anyhow::bail!("{}", e); }
        Ok((self.desc.clone(), self.tags.clone()))
    }
    fn embedding_model_name(&self) -> &'static str { "mock-embed" }
    fn text_embedding(&self, _t: &str) -> anyhow::Result<Option<Vec<f32>>> {
        Ok(Some(vec![0.1, 0.2, 0.3]))
    }
}

fn setup_db_with_pending() -> (Connection, i64) {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, scan_done_at, exif_done_at)
               VALUES ('/tmp/p.jpg', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')", []).unwrap();
    let id = c.last_insert_rowid();
    (c, id)
}

#[test]
fn llm_stage_writes_description_and_objects_with_ai_provenance() {
    let (c, id) = setup_db_with_pending();
    let mock = MockLlm { desc: "A sunset over Rome".into(), tags: vec!["sunset".into(), "rome".into()], fail: None };
    let stage = LlmStage { client: Arc::new(mock), global_prompt_override: None };

    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    let out = stage.process_one(&c, &pending[0], &clk).unwrap();
    assert!(matches!(out, StageOutcome::Ok));

    let (desc, src): (Option<String>, Option<String>) = c.query_row(
        "SELECT description, description_source FROM photos WHERE id=?1",
        rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(desc.as_deref(), Some("A sunset over Rome"));
    assert_eq!(src.as_deref(), Some("ai"));

    let objects: Vec<(String, String)> = c.prepare(
        "SELECT o.name, po.source FROM photo_objects po JOIN objects o ON o.id=po.object_id WHERE po.photo_id=?1"
    ).unwrap().query_map(rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
     .collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(objects.len(), 2);
    assert!(objects.iter().all(|(_, s)| s == "ai"));
}

#[test]
fn llm_stage_does_not_overwrite_user_description() {
    let (c, id) = setup_db_with_pending();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("UPDATE photos SET description='User wrote this',
               description_source='user', description_confirmed_at='2026-05-04T11:00:00Z' WHERE id=?1",
              rusqlite::params![id]).unwrap();

    let mock = MockLlm { desc: "AI version".into(), tags: vec![], fail: None };
    let stage = LlmStage { client: Arc::new(mock), global_prompt_override: None };
    let pending = stage.pending(&c, None, 10).unwrap();
    let _ = stage.process_one(&c, &pending[0], &clk).unwrap();

    let desc: String = c.query_row("SELECT description FROM photos WHERE id=?1",
        rusqlite::params![id], |r| r.get(0)).unwrap();
    assert_eq!(desc, "User wrote this", "user description must not be overwritten");
}

#[test]
fn llm_stage_classifies_unreachable_error() {
    let (c, _id) = setup_db_with_pending();
    let mock = MockLlm { desc: "".into(), tags: vec![], fail: Some("connection refused".into()) };
    let stage = LlmStage { client: Arc::new(mock), global_prompt_override: None };
    let pending = stage.pending(&c, None, 10).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    let out = stage.process_one(&c, &pending[0], &clk).unwrap();
    match out {
        StageOutcome::Err { error_class, .. } => assert_eq!(error_class, "llm_unreachable"),
        _ => panic!("expected error"),
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test --test llm_stage`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/pipeline/stages/llm.rs src/llm/client.rs tests/llm_stage.rs
git commit -m "Implement LLM stage with provenance-aware description/objects writes"
```

---

### Task 17: Index stage

Recomputes smart-album memberships for the photo, marks `index_done_at`. Person centroid recompute is stubbed (Plan 2 fills it in). Smart-album recompute uses `filter_eval::matches_photo`.

**Files:**
- Modify: `src/pipeline/stages/index.rs`

Spec reference: §4.1 row 6, §6.6.

- [ ] **Step 1: Implement**

```rust
//! Index stage: post-process after model stages. Recomputes smart-album
//! membership for THIS photo. Plan 2 will add person-centroid recompute.

use crate::db::clock::Clock;
use crate::db::filter_eval::{matches_photo, Filter};
use crate::pipeline::stages::{
    Stage, StageId, StageOutcome, PendingPhoto, pending_default,
};
use anyhow::Result;
use rusqlite::{params, Connection};

pub struct IndexStage;

impl Stage for IndexStage {
    fn id(&self) -> StageId { StageId::Index }

    fn pending(&self, conn: &Connection, folder: Option<&str>, limit: usize)
        -> Result<Vec<PendingPhoto>>
    {
        // Index runs after thumb, llm and (in Plan 2) faces. For Plan 1 we require
        // exif (the others may have been skipped by config or disabled).
        // Conservative: require all the model stages we have.
        let sql = match folder {
            Some(_) => "SELECT id, path FROM photos
                        WHERE index_done_at IS NULL
                          AND exif_done_at IS NOT NULL
                          AND thumb_done_at IS NOT NULL
                          AND llm_done_at IS NOT NULL
                          AND path LIKE ?1
                        ORDER BY id ASC LIMIT ?2",
            None => "SELECT id, path FROM photos
                     WHERE index_done_at IS NULL
                       AND exif_done_at IS NOT NULL
                       AND thumb_done_at IS NOT NULL
                       AND llm_done_at IS NOT NULL
                     ORDER BY id ASC LIMIT ?1",
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = match folder {
            Some(f) => {
                let prefix = format!("{}/%", f.trim_end_matches('/'));
                stmt.query_map(params![prefix, limit as i64], |r| Ok(PendingPhoto {
                    id: r.get(0)?, path: std::path::PathBuf::from(r.get::<_, String>(1)?),
                }))?.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                stmt.query_map(params![limit as i64], |r| Ok(PendingPhoto {
                    id: r.get(0)?, path: std::path::PathBuf::from(r.get::<_, String>(1)?),
                }))?.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(rows)
    }

    fn process_one(&self, conn: &Connection, photo: &PendingPhoto, _clock: &dyn Clock)
        -> Result<StageOutcome>
    {
        // Load all smart albums and recompute membership for this photo.
        let mut stmt = conn.prepare(
            "SELECT id, filter_json FROM albums
             WHERE kind='smart' AND filter_json IS NOT NULL"
        )?;
        let smart: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let tx = conn.unchecked_transaction()?;
        for (album_id, filter_json) in smart {
            let filter: Filter = match serde_json::from_str(&filter_json) {
                Ok(f) => f,
                Err(_) => continue, // bad filter JSON: skip this album for now
            };
            let matches = matches_photo(&tx, photo.id, &filter)?;
            if matches {
                tx.execute(
                    "INSERT OR IGNORE INTO album_photos(album_id, photo_id) VALUES (?1, ?2)",
                    params![album_id, photo.id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM album_photos WHERE album_id=?1 AND photo_id=?2",
                    params![album_id, photo.id],
                )?;
            }
        }
        tx.commit()?;
        Ok(StageOutcome::Ok)
    }
}
```

- [ ] **Step 2: Integration test (smart-album membership)**

Create `tests/index_stage.rs`:

```rust
use clepho::db::{apply_v2_schema, FixedClock};
use clepho::pipeline::stages::{Stage, StageOutcome, index::IndexStage};
use rusqlite::Connection;

fn setup() -> (Connection, i64) {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // photo with sunset object, all stages but index complete
    c.execute("INSERT INTO photos(path, scan_done_at, exif_done_at, thumb_done_at, llm_done_at, taken_at)
               VALUES ('p.jpg', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z',
                       '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z', '2024-06-12T00:00:00Z')", []).unwrap();
    let id = c.last_insert_rowid();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();
    c.execute("INSERT INTO photo_objects(photo_id, object_id, source) VALUES (?1, 1, 'ai')",
              rusqlite::params![id]).unwrap();
    (c, id)
}

#[test]
fn index_adds_to_matching_smart_album() {
    let (c, id) = setup();
    let filter = r#"{"combinator":"and","clauses":[
        {"facet":"objects","op":"any_of","values":["sunset"]}
    ]}"#;
    c.execute("INSERT INTO albums(name, kind, filter_json) VALUES ('Sunsets','smart',?1)",
              rusqlite::params![filter]).unwrap();
    let album_id = c.last_insert_rowid();

    let stage = IndexStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let _ = stage.process_one(&c, &pending[0], &FixedClock::iso("2026-05-04T12:00:00Z")).unwrap();

    let cnt: i64 = c.query_row(
        "SELECT COUNT(*) FROM album_photos WHERE album_id=?1 AND photo_id=?2",
        rusqlite::params![album_id, id], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 1);
}

#[test]
fn index_removes_from_non_matching_smart_album() {
    let (c, id) = setup();
    let filter = r#"{"combinator":"and","clauses":[
        {"facet":"objects","op":"any_of","values":["snow"]}
    ]}"#;
    c.execute("INSERT INTO albums(name, kind, filter_json) VALUES ('Snowy','smart',?1)",
              rusqlite::params![filter]).unwrap();
    let album_id = c.last_insert_rowid();
    // Pre-seed (simulating prior incorrect membership)
    c.execute("INSERT INTO album_photos(album_id, photo_id) VALUES (?1, ?2)",
              rusqlite::params![album_id, id]).unwrap();

    let stage = IndexStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    let _ = stage.process_one(&c, &pending[0], &FixedClock::iso("2026-05-04T12:00:00Z")).unwrap();

    let cnt: i64 = c.query_row(
        "SELECT COUNT(*) FROM album_photos WHERE album_id=?1 AND photo_id=?2",
        rusqlite::params![album_id, id], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 0);
}
```

- [ ] **Step 3: Run**

Run: `cargo test --test index_stage`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/pipeline/stages/index.rs tests/index_stage.rs
git commit -m "Implement index stage: smart-album membership recompute"
```

---

### Task 18: Config sections — `[pipeline]` and `[logging]`

Replace the temporary stub from Task 11 with the full config sections, plus rename of `batch_concurrency`.

**Files:**
- Modify: `src/config.rs`

Spec reference: §4.5, §8.7.

- [ ] **Step 1: Add full config types**

In `src/config.rs`, replace the temporary `PipelineConfig` stub with:

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub scan_workers: u32,
    pub exif_workers: u32,
    pub thumb_workers: u32,
    pub llm_workers: u32,
    pub faces_workers: u32,
    pub index_workers: u32,
    pub circuit_breaker_threshold: u32,
    pub thumbnail_cache_dir: Option<std::path::PathBuf>,  // overrides default
    pub thumbnail_max_edge: u32,
    pub allowed_extensions: Vec<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            scan_workers: 2, exif_workers: 4, thumb_workers: 4,
            llm_workers: 2, faces_workers: 2, index_workers: 4,
            circuit_breaker_threshold: 3,
            thumbnail_cache_dir: None,
            thumbnail_max_edge: 256,
            allowed_extensions: vec!["jpg","jpeg","png","heic","webp","tiff","tif"].into_iter()
                .map(String::from).collect(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,         // "trace"|"debug"|"info"|"warn"|"error"
    pub retention_days: u64,
    pub log_dir: Option<std::path::PathBuf>,  // overrides XDG_STATE_HOME default
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { level: "info".into(), retention_days: 30, log_dir: None }
    }
}
```

- [ ] **Step 2: Wire into the top-level `Config`**

Find the top-level `Config` struct in `src/config.rs` (it has fields like `[llm]`, `[scanner]`, etc.) and add:

```rust
#[serde(default)]
pub pipeline: PipelineConfig,
#[serde(default)]
pub logging: LoggingConfig,
```

- [ ] **Step 3: Migrate `batch_concurrency`**

In `LlmConfig`, mark the field deprecated and migrate at config-load time. Find the `Config::load` function and after parsing, add:

```rust
// Migrate old [llm].batch_concurrency to [pipeline].llm_workers if user hasn't set the new field.
if cfg.llm.batch_concurrency != 4 /* old default */ && cfg.pipeline.llm_workers == 2 /* default */ {
    cfg.pipeline.llm_workers = cfg.llm.batch_concurrency as u32;
    eprintln!("[config] Migrated llm.batch_concurrency={} → pipeline.llm_workers", cfg.pipeline.llm_workers);
}
```

(If `LlmConfig` doesn't have a default value of 4 for `batch_concurrency`, adjust the check accordingly.)

- [ ] **Step 4: Verify build and test config parsing**

Run: `cargo build --lib && cargo test --lib config`
Expected: builds; existing config tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "Add [pipeline] and [logging] config sections"
```

---

### Task 19: Reset-DB dialog

When the daemon or TUI starts and detects a `Legacy` schema, prompt the user. In headless daemon mode, refuse to start with a clear error pointing to a CLI flag.

**Files:**
- Create: `src/ui/reset_db_dialog.rs`
- Modify: `src/ui/mod.rs` (register dialog)
- Modify: `src/app.rs` (invoke on startup)
- Modify: `src/bin/daemon.rs` (refuse to start; require `--reset-db` flag)

Spec reference: §10.1.

- [ ] **Step 1: TUI dialog**

Create `src/ui/reset_db_dialog.rs`:

```rust
//! Shown once on first startup when the existing DB has the legacy schema (gen<2).
//! User must explicitly confirm to drop tables and apply v2.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetChoice { Pending, Confirmed, Declined }

pub struct ResetDbDialog {
    pub choice: ResetChoice,
}

impl Default for ResetDbDialog {
    fn default() -> Self { Self { choice: ResetChoice::Pending } }
}

impl ResetDbDialog {
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL)
            .title(" Database schema upgrade required ")
            .style(Style::default().fg(Color::Yellow));

        let body = "Clepho v2 uses a new schema. Your existing database will be DROPPED \
                    and recreated empty. Photos must be rescanned. There is no automatic data migration in v2.\n\n\
                    [y] Drop and reset    [n] Quit";

        let p = Paragraph::new(body).block(block).wrap(Wrap { trim: true });
        f.render_widget(Clear, area);
        f.render_widget(p, area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.choice = ResetChoice::Confirmed,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.choice = ResetChoice::Declined,
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Wire into `src/app.rs`**

In the `App` struct, add a field:

```rust
pub reset_db_dialog: Option<crate::ui::reset_db_dialog::ResetDbDialog>,
pub pending_db_reset: bool,
```

In the App initialisation, after opening the DB:

```rust
use crate::db::{detect_schema_state, SchemaState, apply_v2_schema, reset_to_v2};

let conn = /* existing open code */;
match detect_schema_state(&conn)? {
    SchemaState::Empty => { apply_v2_schema(&conn)?; }
    SchemaState::Current => { /* nothing to do */ }
    SchemaState::Legacy => {
        app.reset_db_dialog = Some(Default::default());
        app.mode = AppMode::ResetDb;
        // dialog drives the reset on confirm
    }
    SchemaState::Newer(v) => {
        anyhow::bail!("Database schema is newer ({}) than this binary supports ({}). \
                       Upgrade clepho or restore an older DB.", v, clepho::db::migrate::CURRENT_GENERATION);
    }
}
```

When the dialog returns `Confirmed`, run `reset_to_v2(&conn)` and clear `app.reset_db_dialog`. On `Declined`, exit cleanly.

- [ ] **Step 3: Daemon must refuse to start without explicit flag**

In `src/bin/daemon.rs`, after opening the DB:

```rust
match detect_schema_state(&conn)? {
    SchemaState::Empty   => apply_v2_schema(&conn)?,
    SchemaState::Current => {},
    SchemaState::Legacy  => {
        if !args.reset_db {  // CLI flag added below
            eprintln!("ERROR: Database schema is legacy. Run 'clepho-daemon --reset-db' to reset, \
                       or open the TUI 'clepho' to confirm interactively.");
            std::process::exit(2);
        }
        eprintln!("WARN: Resetting database to v2 (--reset-db flag set).");
        reset_to_v2(&conn)?;
    }
    SchemaState::Newer(v) => {
        eprintln!("ERROR: schema is newer ({}) than supported", v);
        std::process::exit(3);
    }
}
```

Add a `--reset-db` flag to whatever arg parser the daemon uses (likely `clap`).

- [ ] **Step 4: Test**

Add `tests/reset_flow.rs`:

```rust
use clepho::db::{detect_schema_state, SchemaState, apply_v2_schema, reset_to_v2};
use rusqlite::Connection;

#[test]
fn legacy_schema_triggers_reset_path() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE photos (id INTEGER PRIMARY KEY, tags TEXT);
                     INSERT INTO photos(tags) VALUES ('legacy');").unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Legacy);
    reset_to_v2(&c).unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);
}
```

Run: `cargo test --test reset_flow`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/ui/reset_db_dialog.rs src/ui/mod.rs src/app.rs src/bin/daemon.rs tests/reset_flow.rs
git commit -m "Add reset-db dialog (TUI) and --reset-db flag (daemon)"
```

---

### Task 20: Managed folders + folder_prompts CRUD

Plain SQL CRUD around the two new tables. Used by Tasks 21 (daemon) and 24 (TUI key handlers).

**Files:**
- Create: `src/db/managed_folders.rs`
- Modify: `src/db/mod.rs`

Spec reference: §3.6, §4.6.

- [ ] **Step 1: Implement**

Create `src/db/managed_folders.rs`:

```rust
//! CRUD for managed_folders + folder_prompts.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedFolder {
    pub id: i64,
    pub path: String,
    pub schedule_cron: Option<String>,
    pub paused: bool,
    pub last_run_at: Option<String>,
    pub faces_disabled: bool,
}

pub fn list(conn: &Connection) -> Result<Vec<ManagedFolder>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, schedule_cron, paused, last_run_at, faces_disabled
         FROM managed_folders ORDER BY path"
    )?;
    let rows = stmt.query_map([], |r| Ok(ManagedFolder {
        id: r.get(0)?, path: r.get(1)?, schedule_cron: r.get(2)?,
        paused: r.get::<_, i64>(3)? != 0,
        last_run_at: r.get(4)?,
        faces_disabled: r.get::<_, i64>(5)? != 0,
    }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn add(conn: &Connection, path: &str, schedule_cron: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO managed_folders(path, schedule_cron) VALUES (?1, ?2)",
        params![path, schedule_cron],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM managed_folders WHERE path=?1", params![path], |r| r.get(0),
    )?;
    Ok(id)
}

pub fn remove(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM managed_folders WHERE path=?1", params![path])?;
    Ok(())
}

pub fn set_paused(conn: &Connection, path: &str, paused: bool) -> Result<()> {
    conn.execute(
        "UPDATE managed_folders SET paused=?1, updated_at=CURRENT_TIMESTAMP WHERE path=?2",
        params![if paused { 1 } else { 0 }, path],
    )?;
    Ok(())
}

pub fn set_last_run(conn: &Connection, path: &str, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE managed_folders SET last_run_at=?1, updated_at=CURRENT_TIMESTAMP WHERE path=?2",
        params![ts, path],
    )?;
    Ok(())
}

pub fn is_managed(conn: &Connection, path: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM managed_folders WHERE path=?1)",
        params![path], |r| r.get(0)
    )?)
}

// folder_prompts (works for managed AND ad-hoc folders)

pub fn get_prompt(conn: &Connection, path: &str) -> Result<Option<String>> {
    Ok(conn.query_row(
        "SELECT custom_prompt FROM folder_prompts WHERE path=?1",
        params![path], |r| r.get(0),
    ).optional()?)
}

pub fn set_prompt(conn: &Connection, path: &str, prompt: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO folder_prompts(path, custom_prompt) VALUES (?1, ?2)
         ON CONFLICT(path) DO UPDATE SET custom_prompt=?2, updated_at=CURRENT_TIMESTAMP",
        params![path, prompt],
    )?;
    Ok(())
}

pub fn delete_prompt(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM folder_prompts WHERE path=?1", params![path])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::apply_v2_schema;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        apply_v2_schema(&c).unwrap();
        c
    }

    #[test]
    fn add_then_list() {
        let c = db();
        add(&c, "/photos/2024", Some("0 5 * * *")).unwrap();
        add(&c, "/photos/2023", None).unwrap();
        let xs = list(&c).unwrap();
        assert_eq!(xs.len(), 2);
        assert_eq!(xs[0].path, "/photos/2023");
        assert!(xs[1].schedule_cron.is_some());
    }

    #[test]
    fn add_is_idempotent_on_path() {
        let c = db();
        add(&c, "/photos", None).unwrap();
        add(&c, "/photos", None).unwrap();
        assert_eq!(list(&c).unwrap().len(), 1);
    }

    #[test]
    fn pause_and_unpause() {
        let c = db();
        add(&c, "/p", None).unwrap();
        set_paused(&c, "/p", true).unwrap();
        let xs = list(&c).unwrap();
        assert!(xs[0].paused);
        set_paused(&c, "/p", false).unwrap();
        assert!(!list(&c).unwrap()[0].paused);
    }

    #[test]
    fn folder_prompt_set_get_delete() {
        let c = db();
        assert!(get_prompt(&c, "/p").unwrap().is_none());
        set_prompt(&c, "/p", "wedding photos").unwrap();
        assert_eq!(get_prompt(&c, "/p").unwrap().as_deref(), Some("wedding photos"));
        set_prompt(&c, "/p", "updated").unwrap();
        assert_eq!(get_prompt(&c, "/p").unwrap().as_deref(), Some("updated"));
        delete_prompt(&c, "/p").unwrap();
        assert!(get_prompt(&c, "/p").unwrap().is_none());
    }
}
```

- [ ] **Step 2: Wire**

In `src/db/mod.rs`:

```rust
pub mod managed_folders;
```

- [ ] **Step 3: Run**

Run: `cargo test --lib managed_folders::tests`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/db/managed_folders.rs src/db/mod.rs
git commit -m "Add managed_folders and folder_prompts CRUD"
```

---

### Task 21: Daemon driver — scheduler ticks + watcher

Replace existing inline batch logic in `src/bin/daemon.rs` with a scheduler-driven loop. Iterates managed folders, runs scheduler passes, fires filesystem watchers for new files.

**Files:**
- Modify: `src/bin/daemon.rs`
- Create: `src/pipeline/watcher.rs`
- Modify: `src/pipeline/mod.rs` (register `watcher`)

Spec reference: §4.6.

- [ ] **Step 1: Filesystem watcher module**

Create `src/pipeline/watcher.rs`:

```rust
//! Filesystem watcher for managed folders. New files trigger an immediate
//! scan-stage discovery for that folder.

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, EventKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
    pub events: mpsc::Receiver<PathBuf>,
}

impl FolderWatcher {
    pub fn watch(roots: Vec<PathBuf>) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            if let Ok(ev) = res {
                if matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    for p in ev.paths {
                        let _ = tx.send(p);
                    }
                }
            }
        })?;
        for root in &roots {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
        Ok(Self { _watcher: watcher, events: rx })
    }
}
```

- [ ] **Step 2: Daemon loop replacement**

In `src/bin/daemon.rs`, the main loop becomes (sketch):

```rust
fn main() -> anyhow::Result<()> {
    // ... existing CLI parse, config load, DB open, schema check (Task 19) ...

    let conn = /* opened */;
    let log_dir = config.logging.log_dir.clone()
        .unwrap_or_else(|| crate::pipeline::log::JsonlAppender::default_path());
    let log = std::sync::Arc::new(crate::pipeline::log::JsonlAppender::new(log_dir)?);
    log.rotate(config.logging.retention_days)?;

    let breaker = std::sync::Arc::new(
        crate::pipeline::circuit_breaker::CircuitBreaker::new(config.pipeline.circuit_breaker_threshold)
    );

    // Build the LlmClient from existing factory.
    let llm_client = crate::llm::client::create_client(&config.llm)?;

    // Build stages
    let scan = std::sync::Arc::new(crate::pipeline::stages::scan::ScanStage::new(
        crate::db::managed_folders::list(&conn)?
            .into_iter().map(|m| std::path::PathBuf::from(m.path)).collect(),
        config.pipeline.allowed_extensions.clone(),
    ));
    let exif  = std::sync::Arc::new(crate::pipeline::stages::exif::ExifStage);
    let thumb_dir = config.pipeline.thumbnail_cache_dir.clone()
        .unwrap_or_else(|| dirs::cache_dir().unwrap_or_default().join("clepho/thumbs"));
    let thumb = std::sync::Arc::new(crate::pipeline::stages::thumb::ThumbStage::new(
        thumb_dir, config.pipeline.thumbnail_max_edge,
    ));
    let llm   = std::sync::Arc::new(crate::pipeline::stages::llm::LlmStage {
        client: llm_client.clone(),
        global_prompt_override: config.llm.custom_prompt.clone(),
    });
    let index = std::sync::Arc::new(crate::pipeline::stages::index::IndexStage);

    let stages: Vec<std::sync::Arc<dyn crate::pipeline::stages::Stage>> =
        vec![scan.clone(), exif, thumb, llm, index];

    let scheduler = crate::pipeline::scheduler::Scheduler {
        stages, breaker: breaker.clone(), log: log.clone(),
        config: config.pipeline.clone(),
        clock: std::sync::Arc::new(crate::db::SystemClock),
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let folders: Vec<std::path::PathBuf> = crate::db::managed_folders::list(&conn)?
        .into_iter().filter(|f| !f.paused)
        .map(|f| std::path::PathBuf::from(f.path)).collect();
    let watcher = if !folders.is_empty() {
        Some(crate::pipeline::watcher::FolderWatcher::watch(folders.clone())?)
    } else { None };

    // Main loop: tick every N seconds; opportunistic discovery via watcher; cron-driven.
    let tick_secs = 30u64;
    loop {
        // 1. Drain watcher events: discover any new files.
        if let Some(w) = &watcher {
            while let Ok(path) = w.events.try_recv() {
                if path.is_file() {
                    if let Some(parent) = path.parent() {
                        let _ = scan.discover(&conn, parent);
                    }
                }
            }
        }

        // 2. For each managed folder, do one scheduler pass.
        for f in crate::db::managed_folders::list(&conn)? {
            if f.paused { continue; }
            // Cron evaluation deferred to existing schedule code; for now, run on every tick.
            scan.discover(&conn, std::path::Path::new(&f.path)).ok();
            scheduler.run_pass(&conn, Some(&f.path))?;
            crate::db::managed_folders::set_last_run(
                &conn, &f.path,
                &chrono::Utc::now().to_rfc3339()
            )?;
        }

        std::thread::sleep(std::time::Duration::from_secs(tick_secs));
    }
}
```

This is a sketch — adapt to the existing `daemon.rs` structure. Do NOT delete existing code wholesale on first edit; prefer adding the new loop in parallel and removing the old once green.

- [ ] **Step 3: Add `dirs` to Cargo.toml** if missing

Run: `grep '^dirs' Cargo.toml` — if absent, add `dirs = "5"` to `[dependencies]`.

- [ ] **Step 4: Build the daemon binary**

Run: `cargo build --bin clepho-daemon`
Expected: builds.

- [ ] **Step 5: Commit**

```bash
git add src/bin/daemon.rs src/pipeline/watcher.rs src/pipeline/mod.rs Cargo.toml
git commit -m "Replace daemon batch logic with scheduler-driven loop + watcher"
```

---

### Task 22: Pipeline Status screen (MVP)

Replaces the existing `task_list_dialog.rs` content with a three-section view: managed folders, live workers, failures inbox. Plan 4 polishes this; Plan 1 ships the read-mostly MVP.

**Files:**
- Create: `src/ui/pipeline_status.rs`
- Modify: `src/ui/mod.rs` (register; alias key `T` to this)
- Modify: `src/app.rs` (route `T` to new screen)

Spec reference: §4.7, §8.6.

- [ ] **Step 1: Implement screen**

Create `src/ui/pipeline_status.rs`:

```rust
//! Pipeline Status screen (spec §4.7).
//! MVP: managed folders + failures inbox. Workers panel shows last known activity.

use crate::db::managed_folders::ManagedFolder;
use crate::db::pipeline_events::UnresolvedGroup;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

pub struct PipelineStatusScreen {
    pub folders: Vec<ManagedFolder>,
    pub failures: Vec<UnresolvedGroup>,
    pub focus: Focus,
    pub folder_state: ListState,
    pub failure_state: ListState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus { Folders, Failures }

impl PipelineStatusScreen {
    pub fn new(folders: Vec<ManagedFolder>, failures: Vec<UnresolvedGroup>) -> Self {
        let mut s = Self {
            folders, failures, focus: Focus::Folders,
            folder_state: ListState::default(), failure_state: ListState::default(),
        };
        if !s.folders.is_empty() { s.folder_state.select(Some(0)); }
        if !s.failures.is_empty() { s.failure_state.select(Some(0)); }
        s
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let chunks = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(20), Constraint::Percentage(40)])
            .split(area);

        // Managed folders
        let folder_items: Vec<ListItem> = self.folders.iter().map(|f| {
            let badge = if f.paused { "⏸" } else { "●" };
            let cron = f.schedule_cron.as_deref().unwrap_or("—");
            ListItem::new(format!("{} {:60} cron: {:20} last: {}",
                badge, f.path, cron, f.last_run_at.as_deref().unwrap_or("—")))
        }).collect();
        let folders_list = List::new(folder_items)
            .block(Block::default().borders(Borders::ALL).title(" Managed Folders "))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(folders_list, chunks[0], &mut self.folder_state);

        // Workers (placeholder; live activity comes in Plan 4)
        let workers = Paragraph::new("scan: idle  exif: idle  thumb: idle  llm: idle  faces: idle  index: idle\n\
                                      (live activity in Plan 4)")
            .block(Block::default().borders(Borders::ALL).title(" Workers "))
            .wrap(Wrap { trim: true });
        f.render_widget(workers, chunks[1]);

        // Failures inbox
        let failure_items: Vec<ListItem> = self.failures.iter().map(|g| {
            ListItem::new(format!("{} / {} — {} ({} occurrences) — e.g. \"{}\"",
                g.stage.as_deref().unwrap_or("?"),
                g.error_class.as_deref().unwrap_or("?"),
                g.example_message,
                g.count,
                truncate(&g.example_message, 60)))
        }).collect();
        let failures_list = List::new(failure_items)
            .block(Block::default().borders(Borders::ALL).title(" Failures Inbox "))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(failures_list, chunks[2], &mut self.failure_state);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ScreenAction {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let st = self.active_state();
                let len = self.active_len();
                let i = st.selected().unwrap_or(0);
                if i + 1 < len { st.select(Some(i + 1)); }
                ScreenAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let st = self.active_state();
                let i = st.selected().unwrap_or(0);
                if i > 0 { st.select(Some(i - 1)); }
                ScreenAction::None
            }
            KeyCode::Tab => {
                self.focus = match self.focus { Focus::Folders => Focus::Failures, Focus::Failures => Focus::Folders };
                ScreenAction::None
            }
            KeyCode::Char('p') if self.focus == Focus::Folders => {
                if let Some(i) = self.folder_state.selected() {
                    if let Some(f) = self.folders.get(i) {
                        return ScreenAction::TogglePause(f.path.clone());
                    }
                }
                ScreenAction::None
            }
            KeyCode::Char('r') if self.focus == Focus::Failures => {
                if let Some(i) = self.failure_state.selected() {
                    if let Some(g) = self.failures.get(i) {
                        return ScreenAction::RetryGroup {
                            stage: g.stage.clone(), error_class: g.error_class.clone(),
                        };
                    }
                }
                ScreenAction::None
            }
            KeyCode::Char('c') if self.focus == Focus::Failures => {
                if let Some(i) = self.failure_state.selected() {
                    if let Some(g) = self.failures.get(i) {
                        return ScreenAction::ClearGroup {
                            stage: g.stage.clone(), error_class: g.error_class.clone(),
                        };
                    }
                }
                ScreenAction::None
            }
            KeyCode::Esc | KeyCode::Char('q') => ScreenAction::Close,
            _ => ScreenAction::None,
        }
    }

    fn active_state(&mut self) -> &mut ListState {
        match self.focus { Focus::Folders => &mut self.folder_state, Focus::Failures => &mut self.failure_state }
    }
    fn active_len(&self) -> usize {
        match self.focus { Focus::Folders => self.folders.len(), Focus::Failures => self.failures.len() }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n { s.into() } else { format!("{}…", &s[..n]) }
}

#[derive(Debug, Clone)]
pub enum ScreenAction {
    None,
    Close,
    TogglePause(String),
    RetryGroup { stage: Option<String>, error_class: Option<String> },
    ClearGroup { stage: Option<String>, error_class: Option<String> },
}
```

- [ ] **Step 2: Snapshot test**

Add to `tests/pipeline_status_snapshot.rs`:

```rust
use clepho::ui::pipeline_status::PipelineStatusScreen;
use clepho::db::managed_folders::ManagedFolder;
use clepho::db::pipeline_events::UnresolvedGroup;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn pipeline_status_snapshot_baseline() {
    let folders = vec![
        ManagedFolder { id: 1, path: "/photos/2024".into(), schedule_cron: Some("0 5 * * *".into()),
            paused: false, last_run_at: Some("2026-05-04T05:00:00Z".into()), faces_disabled: false },
        ManagedFolder { id: 2, path: "/photos/2023".into(), schedule_cron: None,
            paused: true, last_run_at: None, faces_disabled: false },
    ];
    let failures = vec![
        UnresolvedGroup { stage: Some("llm".into()), error_class: Some("llm_unreachable".into()),
            count: 8, example_message: "connection refused".into() },
    ];

    let backend = TestBackend::new(120, 30);
    let mut term = Terminal::new(backend).unwrap();
    let mut screen = PipelineStatusScreen::new(folders, failures);

    term.draw(|f| screen.render(f, f.area())).unwrap();
    insta::assert_snapshot!(term.backend());
}
```

- [ ] **Step 3: Run**

Run: `cargo test --test pipeline_status_snapshot`
Expected: first run creates snapshot — review with `cargo insta review`.

- [ ] **Step 4: Wire in `app.rs` for the `T` key**

In the `App::handle_key` (or wherever key dispatch happens), route `T` to open `PipelineStatusScreen`. Replace any existing `T` handler that opened `task_list_dialog.rs`. Actions returned by `handle_key` map to:
- `Close` → return to previous mode
- `TogglePause(path)` → call `managed_folders::set_paused`
- `RetryGroup` → clear the relevant `<stage>_done_at` for affected photos, clear `<stage>_error`, and call `breaker.reset(stage)`. Pseudo-SQL: `UPDATE photos SET <stage>_error=NULL WHERE <stage>_error IS NOT NULL` (with predicate filtering by `error_class` if needed — open the matching `pipeline_events` row to find affected `photo_id` values).
- `ClearGroup` → call `pipeline_events::resolve_group`

- [ ] **Step 5: Commit**

```bash
git add src/ui/pipeline_status.rs src/ui/mod.rs src/app.rs tests/pipeline_status_snapshot.rs
git commit -m "Add Pipeline Status screen MVP (folders + failures + actions)"
```

---

### Task 23: Reprocess dialog (`Shift+R`)

Stage-selection dialog with provenance pre-flight count: "Will reprocess N photos. M user-edited and K confirmed-AI values will be preserved."

**Files:**
- Create: `src/ui/reprocess_dialog.rs`
- Modify: `src/app.rs` (route `Shift+R` to open dialog)

Spec reference: §4.4.

- [ ] **Step 1: Implement dialog**

```rust
//! Force-reprocess dialog (spec §4.4). Lets the user choose which stages to clear
//! `<stage>_done_at` for, with a pre-flight count of values that will be preserved
//! by the provenance contract.

use crate::pipeline::stages::StageId;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use rusqlite::Connection;

pub struct ReprocessDialog {
    pub folder: String,
    pub photo_count: i64,
    pub stages: [(StageId, bool); 6],   // (id, selected)
    pub cursor: usize,                  // 0..6 = stage rows; 6 = Cancel; 7 = Confirm
    pub preserved_user_values: i64,
    pub preserved_confirmed_values: i64,
}

impl ReprocessDialog {
    pub fn new(conn: &Connection, folder: String) -> anyhow::Result<Self> {
        // Photo count in folder.
        let prefix = format!("{}/%", folder.trim_end_matches('/'));
        let photo_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE path LIKE ?1",
            rusqlite::params![prefix], |r| r.get(0))?;

        // Preserved counts: user description + confirmed description + user/confirmed facet rows.
        let preserved_user_values: i64 = conn.query_row(
            "SELECT (SELECT COUNT(*) FROM photos WHERE path LIKE ?1
                      AND description_source IN ('user','ai_edited'))
                  + (SELECT COUNT(*) FROM photo_objects po JOIN photos p ON p.id=po.photo_id
                      WHERE p.path LIKE ?1 AND po.source='user')
                  + (SELECT COUNT(*) FROM photo_user_tags pt JOIN photos p ON p.id=pt.photo_id
                      WHERE p.path LIKE ?1 AND pt.source='user')",
            rusqlite::params![prefix], |r| r.get(0))?;

        let preserved_confirmed_values: i64 = conn.query_row(
            "SELECT (SELECT COUNT(*) FROM photo_objects po JOIN photos p ON p.id=po.photo_id
                      WHERE p.path LIKE ?1 AND po.source='ai' AND po.confirmed_at IS NOT NULL)
                  + (SELECT COUNT(*) FROM photo_user_tags pt JOIN photos p ON p.id=pt.photo_id
                      WHERE p.path LIKE ?1 AND pt.source='ai' AND pt.confirmed_at IS NOT NULL)",
            rusqlite::params![prefix], |r| r.get(0))?;

        let stages = [
            (StageId::Scan,  false),
            (StageId::Exif,  false),
            (StageId::Thumb, false),
            (StageId::Llm,   true),    // most common case: rerun LLM with new model
            (StageId::Faces, false),
            (StageId::Index, false),
        ];

        Ok(Self { folder, photo_count, stages, cursor: 0,
                  preserved_user_values, preserved_confirmed_values })
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL)
            .title(format!(" Reprocess folder: {} ", self.folder));

        let mut lines: Vec<Line> = vec![
            Line::from(format!("Photos in folder: {}", self.photo_count)),
            Line::from(""),
            Line::from("Select stages to reset:"),
        ];
        for (i, (stage, sel)) in self.stages.iter().enumerate() {
            let cursor = if self.cursor == i { "▶ " } else { "  " };
            let mark = if *sel { "[✓]" } else { "[ ]" };
            lines.push(Line::from(format!("{}{} {}", cursor, mark, stage.name())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "Will preserve: {} user-edited values, {} confirmed-AI values.",
            self.preserved_user_values, self.preserved_confirmed_values
        )));
        lines.push(Line::from(""));
        let cancel_cur = if self.cursor == 6 { "▶ " } else { "  " };
        let confirm_cur = if self.cursor == 7 { "▶ " } else { "  " };
        lines.push(Line::from(format!("{}[Esc] Cancel    {}[Enter] Run", cancel_cur, confirm_cur)));

        let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ReprocessOutcome {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(7);
                ReprocessOutcome::Pending
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cursor > 0 { self.cursor -= 1; }
                ReprocessOutcome::Pending
            }
            KeyCode::Char(' ') if self.cursor < 6 => {
                self.stages[self.cursor].1 = !self.stages[self.cursor].1;
                ReprocessOutcome::Pending
            }
            KeyCode::Esc | KeyCode::Char('q') => ReprocessOutcome::Cancel,
            KeyCode::Enter if self.cursor == 7 || self.cursor < 6 => {
                ReprocessOutcome::Confirm(
                    self.stages.iter().filter(|(_, s)| *s).map(|(id, _)| *id).collect()
                )
            }
            _ => ReprocessOutcome::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReprocessOutcome {
    Pending,
    Cancel,
    Confirm(Vec<StageId>),
}

/// Apply the reset: clear `<stage>_done_at` for the chosen stages on photos in the folder.
/// Provenance is honoured at write time by the stage executors; the reset itself only
/// flips the gate columns.
pub fn apply_reset(
    conn: &Connection,
    folder: &str,
    stages: &[StageId],
) -> anyhow::Result<usize> {
    let prefix = format!("{}/%", folder.trim_end_matches('/'));
    let mut total = 0usize;
    for s in stages {
        let n = conn.execute(
            &format!("UPDATE photos SET {}=NULL, {}=NULL WHERE path LIKE ?1",
                     s.done_col(), s.error_col()),
            rusqlite::params![prefix],
        )?;
        total += n;
        // Always clear index too — its inputs may have changed.
        if *s != StageId::Index {
            conn.execute(
                "UPDATE photos SET index_done_at=NULL WHERE path LIKE ?1",
                rusqlite::params![prefix],
            )?;
        }
    }
    Ok(total)
}
```

- [ ] **Step 2: Test apply_reset preserves user/confirmed values**

Create `tests/force_reprocess.rs`:

```rust
use clepho::db::{apply_v2_schema, FixedClock, FacetTable,
                pipeline_write_facet, user_add_facet, user_confirm_facet};
use clepho::pipeline::stages::StageId;
use clepho::ui::reprocess_dialog::apply_reset;
use rusqlite::Connection;

#[test]
fn reset_clears_done_but_preserves_user_and_confirmed_values() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");

    // Seed two photos in /photos.
    for i in 1..=2 {
        c.execute(&format!("INSERT INTO photos(path, scan_done_at, exif_done_at, llm_done_at, index_done_at,
                                              description, description_source, description_confirmed_at)
                            VALUES ('/photos/p{}.jpg', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                                    {})",
            i,
            if i == 1 { "'AI desc', 'ai', NULL" } else { "'User desc', 'user', '2026-01-02T00:00:00Z'" }
        ), []).unwrap();
    }
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();
    // Photo 1: AI object (unconfirmed) — should be cleared by rerun
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    // Photo 2: user object — must be preserved
    user_add_facet(&c, FacetTable::PhotoObjects, 2, 1, &clk).unwrap();

    apply_reset(&c, "/photos", &[StageId::Llm]).unwrap();

    // Both photos: llm_done_at and index_done_at cleared.
    let llm_done: Vec<Option<String>> = c.prepare("SELECT llm_done_at FROM photos ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert!(llm_done.iter().all(|x| x.is_none()), "all llm_done_at should be NULL after reset");

    // Photo 2's user description and user object: untouched.
    let (desc, src): (String, String) = c.query_row(
        "SELECT description, description_source FROM photos WHERE id=2", [],
        |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(desc, "User desc");
    assert_eq!(src, "user");

    let (osrc, conf): (String, Option<String>) = c.query_row(
        "SELECT source, confirmed_at FROM photo_objects WHERE photo_id=2 AND object_id=1", [],
        |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(osrc, "user");
    assert!(conf.is_some());

    // Photo 1's AI-unconfirmed object row still exists (apply_reset doesn't delete facet rows;
    // the LLM stage re-running will UPDATE/leave it alone via provenance).
    let count: i64 = c.query_row(
        "SELECT COUNT(*) FROM photo_objects WHERE photo_id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}
```

Run: `cargo test --test force_reprocess`
Expected: 1 test passes.

- [ ] **Step 3: Wire `Shift+R` in `app.rs`**

In key dispatch, when `Shift+R` is pressed in folder/browse mode, open `ReprocessDialog::new(&conn, current_folder.clone())?` and route the outcome. On `Confirm(stages)`, call `apply_reset(&conn, &folder, &stages)?`, then trigger an immediate scheduler pass for that folder.

- [ ] **Step 4: Commit**

```bash
git add src/ui/reprocess_dialog.rs src/app.rs tests/force_reprocess.rs
git commit -m "Add force-reprocess dialog with provenance pre-flight"
```

---

### Task 24: BrowseView key handlers (`R`, `M`)

Minimal key wiring for the still-existing browser. (Plan 4 unifies BrowseView/Gallery; Plan 1 just adds these three keys to the existing browser.)

**Files:**
- Modify: `src/app.rs`

Spec reference: §4.6.

- [ ] **Step 1: Add `R` (run pipeline) handler**

In whatever key-dispatch function handles the existing browser (likely `App::handle_key_browser` or similar), add:

```rust
// `R` — ad-hoc pipeline run on the current folder.
KeyCode::Char('R') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
    let folder = self.current_folder().to_path_buf();
    self.spawn_ad_hoc_run(folder);
}

// `Shift+R` — force-reprocess dialog (Task 23 wiring).
KeyCode::Char('R') if key.modifiers.contains(KeyModifiers::SHIFT) => {
    let folder = self.current_folder().to_string_lossy().into_owned();
    self.reprocess_dialog = Some(crate::ui::reprocess_dialog::ReprocessDialog::new(&self.conn, folder)?);
    self.mode = AppMode::Reprocessing;
}

// `M` — toggle managed status of the current folder.
KeyCode::Char('M') => {
    let folder = self.current_folder().to_string_lossy().into_owned();
    if crate::db::managed_folders::is_managed(&self.conn, &folder)? {
        crate::db::managed_folders::remove(&self.conn, &folder)?;
        self.toast("Folder unmanaged");
    } else {
        crate::db::managed_folders::add(&self.conn, &folder, None)?;
        self.toast(&format!("Folder added to managed: {}", folder));
    }
}
```

- [ ] **Step 2: Implement `spawn_ad_hoc_run`**

Add to `App` impl:

```rust
fn spawn_ad_hoc_run(&self, folder: std::path::PathBuf) {
    let folder_str = folder.to_string_lossy().into_owned();
    let scheduler = self.scheduler.clone();
    let conn_path = self.config.database.sqlite_path.clone();
    std::thread::spawn(move || {
        let conn = match rusqlite::Connection::open(&conn_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        // Run discover and one scheduler pass on this folder.
        let _ = scheduler.stages.iter()
            .find_map(|s| if s.id() == clepho::pipeline::stages::StageId::Scan {
                Some(s.clone())
            } else { None })
            .map(|s| {
                let scan: &dyn std::any::Any = s.as_ref() as &dyn std::any::Any;
                if let Some(scan_stage) = scan.downcast_ref::<clepho::pipeline::stages::scan::ScanStage>() {
                    let _ = scan_stage.discover(&conn, std::path::Path::new(&folder_str));
                }
            });
        let _ = scheduler.run_pass(&conn, Some(&folder_str));
    });
}
```

(Adapt connection-acquisition to the existing pattern in clepho — likely a connection pool or a per-call `open` against the configured path.)

- [ ] **Step 3: Add a `toast` helper**

If no toast mechanism exists yet, add a string field `App.transient_status: Option<(String, std::time::Instant)>` and render it from the status bar for ~3s.

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: builds.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "Wire R, Shift+R, M keys for pipeline run/reprocess/manage"
```

---

### Task 25: Skip-if-done + idempotency + crash-resume integration tests

Three load-bearing properties of the scheduler tested as cross-stage integration tests with mocked LLM and a real in-memory DB.

**Files:**
- Create: `tests/skip_if_done.rs`
- Create: `tests/stage_idempotency.rs`
- Create: `tests/resume_after_crash.rs`

Spec reference: §4.3, §4.8, §9.1.

- [ ] **Step 1: Skip-if-done test**

Create `tests/skip_if_done.rs`:

```rust
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{Stage, StageId};
use clepho::pipeline::stages::exif::ExifStage;
use rusqlite::Connection;

#[test]
fn already_done_photos_are_skipped() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, scan_done_at, exif_done_at)
               VALUES ('/p/a.jpg', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                      ('/p/b.jpg', '2026-01-01T00:00:00Z', NULL)", []).unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 100).unwrap();
    let paths: Vec<String> = pending.iter().map(|p| p.path.to_string_lossy().into_owned()).collect();
    assert_eq!(paths, vec!["/p/b.jpg"], "only photo b is pending exif");
}

#[test]
fn pending_respects_prereq() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // Photo with NO scan done — must NOT be returned for exif.
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", []).unwrap();
    let stage = ExifStage;
    let pending = stage.pending(&c, None, 100).unwrap();
    assert!(pending.is_empty(), "exif requires scan_done_at IS NOT NULL");
}
```

Run: `cargo test --test skip_if_done`
Expected: 2 tests pass.

- [ ] **Step 2: Idempotency test**

Create `tests/stage_idempotency.rs`:

```rust
use clepho::db::{apply_v2_schema, FixedClock, FacetTable, pipeline_write_facet};
use rusqlite::Connection;

#[test]
fn pipeline_writing_same_object_twice_is_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", []).unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();

    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();

    let count: i64 = c.query_row("SELECT COUNT(*) FROM photo_objects", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "no duplicate rows from idempotent writes");
}

#[test]
fn rerunning_full_pipeline_on_done_photo_is_no_op() {
    use clepho::pipeline::stages::{Stage, exif::ExifStage};
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, scan_done_at, exif_done_at, taken_at)
               VALUES ('/p/a.jpg', '2026-01-01', '2026-01-01', '2024-06-12T00:00:00Z')", []).unwrap();

    // Re-running exif: pending() returns nothing, so process_one is never called.
    let pending = ExifStage.pending(&c, None, 10).unwrap();
    assert!(pending.is_empty());
    let taken: Option<String> = c.query_row("SELECT taken_at FROM photos WHERE id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(taken.as_deref(), Some("2024-06-12T00:00:00Z"));
}
```

Run: `cargo test --test stage_idempotency`
Expected: 2 tests pass.

- [ ] **Step 3: Resume-after-crash test**

Create `tests/resume_after_crash.rs`:

```rust
use clepho::db::{apply_v2_schema, FixedClock};
use clepho::pipeline::stages::{Stage, StageId, StageOutcome, exif::ExifStage, mark_done, mark_error};
use rusqlite::Connection;

#[test]
fn interrupted_stage_leaves_done_at_null_so_retry_picks_it_up() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path, scan_done_at) VALUES ('/p/a.jpg', '2026-01-01')", []).unwrap();

    // Simulate: stage process_one panicked / process killed before mark_done.
    // We deliberately do NOT call mark_done. Photo's exif_done_at is NULL.
    let after_pending = ExifStage.pending(&c, None, 10).unwrap();
    assert_eq!(after_pending.len(), 1, "photo is still pending after crash");
}

#[test]
fn double_run_of_same_stage_does_not_duplicate_facet_rows() {
    use clepho::db::{FacetTable, pipeline_write_facet};
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", []).unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", []).unwrap();

    // Stage "ran twice" — write the same row twice.
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();

    let n: i64 = c.query_row("SELECT COUNT(*) FROM photo_objects", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
}
```

Run: `cargo test --test resume_after_crash`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/skip_if_done.rs tests/stage_idempotency.rs tests/resume_after_crash.rs
git commit -m "Test skip-if-done, stage idempotency, and crash-resume"
```

---

### Task 26: Circuit breaker integration test (with scheduler)

End-to-end test: scheduler with a deliberately-failing stage trips the breaker after 3 same-class failures, leaves other stages running, and resumes after `breaker.reset`.

**Files:**
- Create: `tests/circuit_breaker_integration.rs`

Spec reference: §8.4.

- [ ] **Step 1: Implement test**

```rust
use clepho::config::PipelineConfig;
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::circuit_breaker::CircuitBreaker;
use clepho::pipeline::log::JsonlAppender;
use clepho::pipeline::scheduler::Scheduler;
use clepho::pipeline::stages::{Stage, StageId, StageOutcome, PendingPhoto, pending_default};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

struct AlwaysFailLlm;
impl Stage for AlwaysFailLlm {
    fn id(&self) -> StageId { StageId::Llm }
    fn pending(&self, c: &Connection, f: Option<&str>, n: usize) -> anyhow::Result<Vec<PendingPhoto>> {
        pending_default(c, "llm_done_at", Some("exif_done_at"), f, n)
    }
    fn process_one(&self, _c: &Connection, _p: &PendingPhoto, _clk: &dyn clepho::db::Clock)
        -> anyhow::Result<StageOutcome>
    {
        Ok(StageOutcome::Err {
            error_class: "llm_unreachable".into(),
            message: "connection refused".into(),
        })
    }
}

#[test]
fn three_same_class_failures_pause_stage() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    for i in 1..=10 {
        c.execute(&format!(
            "INSERT INTO photos(path, scan_done_at, exif_done_at)
             VALUES ('/p/p{}.jpg', '2026-01-01', '2026-01-01')", i), []).unwrap();
    }

    let log = Arc::new(JsonlAppender::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap());
    let breaker = Arc::new(CircuitBreaker::new(3));
    let cfg = PipelineConfig::default();
    let sched = Scheduler {
        stages: vec![Arc::new(AlwaysFailLlm)],
        breaker: breaker.clone(),
        log,
        config: cfg,
        clock: Arc::new(SystemClock),
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let _report = sched.run_pass(&c, None).unwrap();
    assert!(breaker.is_paused(StageId::Llm), "breaker should be tripped after ≥3 failures");

    let pending_events: i64 = c.query_row(
        "SELECT COUNT(*) FROM pipeline_events WHERE level='error' AND stage='llm'",
        [], |r| r.get(0)).unwrap();
    assert!(pending_events >= 3, "got {} error events", pending_events);
}

#[test]
fn breaker_reset_resumes_stage() {
    let breaker = CircuitBreaker::new(2);
    breaker.record_failure(StageId::Llm, "x");
    breaker.record_failure(StageId::Llm, "x");
    assert!(breaker.is_paused(StageId::Llm));
    breaker.reset(StageId::Llm);
    assert!(!breaker.is_paused(StageId::Llm));
}
```

- [ ] **Step 2: Run**

Run: `cargo test --test circuit_breaker_integration`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/circuit_breaker_integration.rs
git commit -m "Test circuit breaker trips and resumes correctly under scheduler"
```

---

### Task 27: Test fixtures

Build a minimal but real fixture set for stage tests that need actual JPEGs with EXIF.

**Files:**
- Create: `tests/fixtures/photos/manifest.json`
- Create: `tests/fixtures/photos/build.rs` (one-time generator)
- Create: `tests/fixtures/photos/*.jpg` (committed outputs)

- [ ] **Step 1: Write the generator**

Create `tests/fixtures/photos/build.rs`:

```rust
//! One-shot fixture builder. Run with `cargo run --example build_fixtures` after
//! adding it as an example in Cargo.toml. Generates tiny JPEGs and writes EXIF
//! tags via `little_exif` (add as dev-dep if running this).
//!
//! Outputs are committed to the repo. This script is for regenerating, not for CI.

fn main() {
    use image::{ImageBuffer, Rgb};
    let dir = std::path::Path::new("tests/fixtures/photos");
    std::fs::create_dir_all(dir).unwrap();

    let solid = |r: u8, g: u8, b: u8| -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        ImageBuffer::from_fn(32, 32, |_, _| Rgb([r, g, b]))
    };

    solid(255, 100, 0).save(dir.join("rome_2024_06_12.jpg")).unwrap();
    solid(50, 150, 200).save(dir.join("florence_2024_06_15.jpg")).unwrap();
    solid(100, 100, 100).save(dir.join("no_exif.jpg")).unwrap();
    solid(200, 200, 200).save(dir.join("no_gps.jpg")).unwrap();
    // EXIF injection: out of scope for v1 — manifest reflects what we actually have.
    // Tests that need EXIF should be marked #[ignore] until fixtures are EXIF-rich,
    // or use synthesised JPEGs without EXIF and verify the no-EXIF path.

    let manifest = serde_json::json!({
        "note": "synthesised JPEGs without EXIF; tests asserting EXIF presence are #[ignore] for now",
        "photos": [
            {"file": "rome_2024_06_12.jpg", "has_exif": false},
            {"file": "florence_2024_06_15.jpg", "has_exif": false},
            {"file": "no_exif.jpg", "has_exif": false},
            {"file": "no_gps.jpg", "has_exif": false},
        ]
    });
    std::fs::write(dir.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    println!("Fixtures generated in {:?}", dir);
}
```

- [ ] **Step 2: Generate (one-time)**

Add temporarily to `Cargo.toml` under `[[example]]`:

```toml
[[example]]
name = "build_fixtures"
path = "tests/fixtures/photos/build.rs"
```

Run: `cargo run --example build_fixtures`
Expected: files appear under `tests/fixtures/photos/`.

- [ ] **Step 3: Mark EXIF-asserting tests `#[ignore]`**

In `tests/exif_stage.rs`, mark the test that depends on real EXIF as `#[ignore]` with a comment pointing to a follow-up issue: "Re-enable when fixtures gain EXIF tags (use little_exif crate)."

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/ Cargo.toml
git commit -m "Add minimal photo fixtures (no EXIF; full EXIF deferred)"
```

---

### Task 28: LLM response parser fixtures + tests

Existing `LlmClient::describe_and_tag_image` has a three-tier parser (direct JSON / code-fenced JSON / legacy TAGS:). Lock its behaviour with fixture-driven tests so it can be safely changed.

**Files:**
- Create: `tests/fixtures/llm_responses/{valid.json, code_fenced.json, legacy_tags.txt, malformed.txt, empty_tags.json}`
- Create: `tests/llm_parser.rs`

- [ ] **Step 1: Write the fixtures**

`tests/fixtures/llm_responses/valid.json`:
```json
{"description": "A sunset over Rome.", "tags": ["sunset","rome","architecture"]}
```

`tests/fixtures/llm_responses/code_fenced.json`:
````
```json
{"description": "A sunset over Rome.", "tags": ["sunset","rome"]}
```
````

`tests/fixtures/llm_responses/legacy_tags.txt`:
```
A sunset over Rome.

TAGS: sunset, rome, architecture
```

`tests/fixtures/llm_responses/malformed.txt`:
```
not json, no TAGS line, just prose.
```

`tests/fixtures/llm_responses/empty_tags.json`:
```json
{"description": "Nothing notable.", "tags": []}
```

- [ ] **Step 2: Test parser directly**

Open `src/llm/client.rs` and find the function that parses the raw response body (call it `parse_describe_and_tag_response`). If it's not factored out, factor it now:

```rust
pub fn parse_describe_and_tag_response(raw: &str) -> (String, Vec<String>) {
    // tier 1: direct JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let (Some(d), Some(t)) = (v.get("description").and_then(|x| x.as_str()),
                                     v.get("tags").and_then(|x| x.as_array())) {
            let tags = t.iter().filter_map(|x| x.as_str()).map(String::from).collect();
            return (d.to_string(), tags);
        }
    }
    // tier 2: code-fenced JSON
    if let Some(stripped) = strip_code_fence(raw) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(stripped) {
            if let (Some(d), Some(t)) = (v.get("description").and_then(|x| x.as_str()),
                                         v.get("tags").and_then(|x| x.as_array())) {
                let tags = t.iter().filter_map(|x| x.as_str()).map(String::from).collect();
                return (d.to_string(), tags);
            }
        }
    }
    // tier 3: legacy TAGS:
    for line_start in ["\nTAGS:", "\n**TAGS:**"] {
        if let Some(idx) = raw.find(line_start) {
            let desc = raw[..idx].trim().to_string();
            let tags_line = &raw[idx + line_start.len()..];
            let tags = tags_line.lines().next().unwrap_or("")
                .split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
            return (desc, tags);
        }
    }
    (raw.to_string(), vec![])
}

fn strip_code_fence(s: &str) -> Option<&str> {
    let s = s.trim();
    let s = s.strip_prefix("```json")?.trim_start();
    s.strip_suffix("```").map(|x| x.trim())
}
```

- [ ] **Step 3: Add the parser tests**

Create `tests/llm_parser.rs`:

```rust
use clepho::llm::client::parse_describe_and_tag_response;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/llm_responses").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing fixture: {:?}", p))
}

#[test]
fn parses_direct_json() {
    let (d, t) = parse_describe_and_tag_response(&fixture("valid.json"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset","rome","architecture"]);
}

#[test]
fn parses_code_fenced_json() {
    let (d, t) = parse_describe_and_tag_response(&fixture("code_fenced.json"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset","rome"]);
}

#[test]
fn parses_legacy_tags_format() {
    let (d, t) = parse_describe_and_tag_response(&fixture("legacy_tags.txt"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset","rome","architecture"]);
}

#[test]
fn malformed_returns_full_text_and_empty_tags() {
    let raw = fixture("malformed.txt");
    let (d, t) = parse_describe_and_tag_response(&raw);
    assert_eq!(d.trim(), raw.trim());
    assert!(t.is_empty());
}

#[test]
fn empty_tags_array_is_preserved() {
    let (d, t) = parse_describe_and_tag_response(&fixture("empty_tags.json"));
    assert_eq!(d, "Nothing notable.");
    assert!(t.is_empty());
}
```

- [ ] **Step 4: Run**

Run: `cargo test --test llm_parser`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/llm_responses/ tests/llm_parser.rs src/llm/client.rs
git commit -m "Lock LLM response parser behaviour with fixture tests"
```

---

### Task 28b: Complete the daemon driver (deferred from Task 21)

Task 21 shipped the `FolderWatcher` module but explicitly deferred replacing the daemon's main loop because three integration prerequisites were missing. After Tasks 22–28, all the pieces — stages, scheduler, log appender, watcher, fixtures, parser tests — exist. This task wires them together and retires the v1 batch-processing path.

By the time this task runs, every stage and every supporting module already has its own tests. This task adds **one** end-to-end integration test against a real SQLite file plus a synthetic photo tree; the rest of the work is plumbing.

**Files:**
- Modify: `src/llm/client.rs` (add per-call-prompt method)
- Modify: `src/db/sqlite.rs` (expose raw connection accessor — SQLite backend only)
- Modify: `src/db/mod.rs` (forward the accessor through `Database`)
- Modify: `src/db/sqlite.rs` (make `initialize()` skip v1 SCHEMA when v2 detected)
- Create: `src/pipeline/llm_adapter.rs` (`impl LlmDescribeClient for LlmClient` adapter)
- Modify: `src/pipeline/mod.rs` (register `pub mod llm_adapter;`)
- Modify: `src/bin/daemon.rs` (replace `process_pending_tasks` / `run_daemon_loop`)
- Create: `tests/daemon_e2e.rs`

Spec reference: §4.5, §4.6, §10.1.

#### Prerequisites: three small refactors

- [ ] **Step 1: Per-call prompt on `LlmClient`**

In `src/llm/client.rs`, add:

```rust
impl LlmClient {
    /// Like `describe_and_tag_image` but lets the caller override the custom
    /// prompt for this single call. Cheaper than rebuilding the entire client
    /// when the prompt changes per folder.
    pub fn describe_and_tag_image_with_prompt(
        &self,
        image_path: &Path,
        custom_prompt: Option<&str>,
    ) -> Result<(String, Vec<String>)> {
        // The provider currently bakes the prompt at construction time. Two
        // viable shapes:
        //   (a) Add a per-call prompt parameter to LlmProvider::describe_image
        //       and thread it through every provider implementation.
        //   (b) Build a temporary provider with the override and call through
        //       the same three-tier parsing this method already uses.
        // (a) is cleaner; pick it if the provider trait is small. Otherwise (b)
        // is correct but allocates a provider per call.
        todo!("implement once you've inspected provider.rs")
    }
}
```

The plan does not pre-decide between (a) and (b). The author should pick after re-reading `src/llm/provider.rs` — if the trait has fewer than five methods, (a) is the right call; otherwise (b) is fine since LLM calls dwarf any allocation cost.

- [ ] **Step 2: `Database` exposes a raw SQLite connection**

In `src/db/sqlite.rs`:

```rust
impl SqliteDb {
    pub fn raw_conn(&self) -> &rusqlite::Connection {
        // Existing field name; check the impl. Likely `&self.conn` or
        // accessing through the connection pool's lock.
        &self.conn
    }
}
```

In `src/db/mod.rs`, add a method on `Database` that only works for the SQLite backend:

```rust
impl Database {
    /// Returns the underlying SQLite connection for callers that need to drive
    /// the v2 pipeline directly. Returns None when running on Postgres.
    pub fn raw_sqlite_conn(&self) -> Option<&rusqlite::Connection> {
        match &self.inner {
            DatabaseInner::Sqlite(db) => Some(db.raw_conn()),
            #[cfg(feature = "postgres")]
            DatabaseInner::Postgres(_) => None,
        }
    }
}
```

The pipeline is SQLite-only in Plan 1. Postgres support for the v2 schema is a separate (later) project.

- [ ] **Step 3: `Database::initialize()` is a no-op on v2 DBs**

In `src/db/sqlite.rs::SqliteDb::initialize`, check `schema_version` first:

```rust
pub fn initialize(&self) -> Result<()> {
    use crate::db::migrate::{detect_schema_state, SchemaState};
    if matches!(detect_schema_state(self.raw_conn())?, SchemaState::Current | SchemaState::Newer(_)) {
        // v2 schema already in place (Task 19's preflight applied it).
        return Ok(());
    }
    // Existing v1 SCHEMA / MIGRATIONS path stays here for backward compat
    // until Task 29 removes it.
    /* existing body */
}
```

This means: when the daemon's preflight (Task 19) applies v2, the subsequent `db.initialize()` is a no-op. When run against a v1 DB without `--reset-db`, the preflight already exited with code 2 — so this branch is unreachable. The fallback continues to handle pure-v1 operation for the TUI on legacy DBs.

#### The real work

- [ ] **Step 4: `LlmClient` adapter for `LlmDescribeClient`**

Create `src/pipeline/llm_adapter.rs`:

```rust
//! Bridges the existing crate::llm::LlmClient to the narrower
//! LlmDescribeClient trait the LlmStage uses.

use crate::llm::client::LlmClient;
use crate::pipeline::stages::llm::LlmDescribeClient;
use anyhow::Result;
use std::path::Path;

pub struct LlmClientAdapter(pub LlmClient);

impl LlmDescribeClient for LlmClientAdapter {
    fn describe_and_tag_image(
        &self,
        image_path: &Path,
        custom_prompt: Option<&str>,
    ) -> Result<(String, Vec<String>)> {
        self.0.describe_and_tag_image_with_prompt(image_path, custom_prompt)
    }

    fn text_embedding(&self, text: &str) -> Result<Option<Vec<f32>>> {
        if !self.0.supports_embeddings() {
            return Ok(None);
        }
        self.0.get_text_embedding(text).map(Some)
    }

    fn embedding_model_name(&self) -> &'static str {
        // The existing LlmClient doesn't expose this string; if you can't
        // reach it cheaply, return "" — pipeline_events captures the model in
        // the embeddings.model column from this getter, so a missing label
        // is recoverable but logged.
        ""
    }
}
```

In `src/pipeline/mod.rs`:

```rust
pub mod llm_adapter;
```

- [ ] **Step 5: Replace the daemon's main loop**

Rewrite `src/bin/daemon.rs::run_daemon_loop` and `process_pending_tasks` to be scheduler-driven. The shape from Task 21's sketch (lines 3909–3988 of this plan) is the spec; the only changes from that sketch are:

- Use `db.raw_sqlite_conn().expect("v2 daemon requires SQLite")` instead of a parallel rusqlite open.
- Wrap the `LlmClient` in `LlmClientAdapter` before passing to `LlmStage`.
- Drop the v1 `process_pending_tasks` body — it becomes `unreachable!()` once Task 29 deletes the `scheduled_tasks` shape, but for this task just stop calling it.

Keep the existing CLI flag handling (`--once`, `--interval`, `--config`, `--reset-db`) unchanged.

- [ ] **Step 6: Wire `T` and the new TUI screen**

Now that the daemon owns the `CircuitBreaker`, the TUI's `PipelineStatusScreen::ScreenAction::RetryGroup` finally has somewhere to send the breaker reset. Two options:

- **(a) IPC-free**: TUI has its own `Arc<CircuitBreaker>` (separate from daemon). RetryGroup just clears the affected `<stage>_error` columns and `<stage>_done_at` markers in the DB; the daemon's breaker reset happens organically on the next observed success.
- **(b) Shared state**: a small `pipeline_status` table the daemon writes its breaker state into, and the TUI's RetryGroup writes a "please reset" intent the daemon polls on next tick.

(a) is simpler and matches the "daemon and TUI talk through the DB" principle from spec §1.1. Pick (a) unless something forces otherwise.

In `src/app.rs`:
- Replace the `T → AppMode::TaskList` route with `T → AppMode::PipelineStatus`.
- Construct `PipelineStatusScreen` on entry with the current `managed_folders::list` and `pipeline_events::unresolved_groups`.
- Handle `ScreenAction::TogglePause`, `RetryGroup`, `ClearGroup`, `Close` by mutating the DB and refreshing the screen state.

#### Verification

- [ ] **Step 7: End-to-end test**

Create `tests/daemon_e2e.rs`. Build a synthetic photo tree, configure the scheduler, hit `Scheduler::run_pass` against the connection, and assert all six stages walk a photo to completion.

```rust
//! End-to-end: photo tree → scheduler → all stages done.
//! Uses MockLlmDescribeClient so the test runs offline.

use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::circuit_breaker::CircuitBreaker;
use clepho::pipeline::log::JsonlAppender;
use clepho::pipeline::scheduler::Scheduler;
use clepho::pipeline::stages::{
    exif::ExifStage, index::IndexStage, llm::{LlmDescribeClient, LlmStage},
    scan::ScanStage, thumb::ThumbStage, Stage,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

struct MockLlm;
impl LlmDescribeClient for MockLlm {
    fn describe_and_tag_image(
        &self, _: &Path, _: Option<&str>,
    ) -> anyhow::Result<(String, Vec<String>)> {
        Ok(("a sunset".into(), vec!["sunset".into()]))
    }
}

#[test]
fn daemon_pipeline_walks_photo_to_index_done() {
    let src = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();

    // Write one fake jpeg
    let img: image::ImageBuffer<image::Rgb<u8>, _> =
        image::ImageBuffer::from_fn(32, 32, |_, _| image::Rgb([200u8, 100, 50]));
    let path = src.path().join("a.jpg");
    img.save(&path).unwrap();

    let conn = Connection::open_in_memory().unwrap();
    apply_v2_schema(&conn).unwrap();

    let scan = Arc::new(ScanStage::new(
        vec![src.path().to_path_buf()],
        vec!["jpg".into()],
    ));
    scan.discover(&conn, src.path()).unwrap();

    let stages: Vec<Arc<dyn Stage>> = vec![
        scan.clone(),
        Arc::new(ExifStage),
        Arc::new(ThumbStage::new(cache.path().to_path_buf(), 64)),
        Arc::new(LlmStage { client: Arc::new(MockLlm), global_prompt_override: None }),
        Arc::new(IndexStage),
    ];

    let scheduler = Scheduler {
        stages,
        breaker: Arc::new(CircuitBreaker::new(3)),
        log: Arc::new(JsonlAppender::new(logs.path()).unwrap()),
        config: clepho::config::PipelineConfig::default(),
        clock: Arc::new(SystemClock),
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    // Run enough passes to walk through every stage. Each pass advances
    // exactly the stages whose prereqs are met; five stages → at most five
    // passes from a fresh row.
    for _ in 0..6 {
        scheduler.run_pass(&conn, None).unwrap();
    }

    let (s, e, t, l, i): (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) =
        conn.query_row(
            "SELECT scan_done_at, exif_done_at, thumb_done_at, llm_done_at, index_done_at
             FROM photos WHERE path LIKE '%a.jpg'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        ).unwrap();
    assert!(s.is_some() && e.is_some() && t.is_some() && l.is_some() && i.is_some(),
        "all stages should be done; got scan={:?} exif={:?} thumb={:?} llm={:?} index={:?}",
        s, e, t, l, i);
}
```

- [ ] **Step 8: Manual daemon smoke**

Run `cargo build --bin clepho-daemon`, then against a temp config + a small photo dir:

```bash
target/debug/clepho-daemon --config /tmp/clepho-test.toml --once
```

Confirm the stages all advance (peek at the DB or the JSONL log) and the daemon exits cleanly.

- [ ] **Step 9: Commit**

```bash
git add src/llm/client.rs src/db/sqlite.rs src/db/mod.rs \
        src/pipeline/llm_adapter.rs src/pipeline/mod.rs \
        src/bin/daemon.rs src/app.rs \
        tests/daemon_e2e.rs
git commit -m "Complete daemon driver: scheduler-driven loop with watcher"
```

---

### Task 29: Cleanup — delete obsolete code

Remove code superseded by Plan 1. Old gallery + tag/edit dialogs stay (Plan 4 retires them); old LLM queue and `directory_prompts` table go now.

**Files:**
- Delete: `src/llm/queue.rs`
- Modify: `src/llm/mod.rs` (remove `pub mod queue;`)
- Modify: any callers of `LlmQueue` — replace with scheduler invocations

- [ ] **Step 1: Find queue callers**

Run: `grep -rn 'LlmQueue\|llm::queue' src/`
Expected: list of files. Replace each call with the scheduler equivalent (mostly already done in `app.rs` Task 24's `spawn_ad_hoc_run`).

- [ ] **Step 2: Delete the file**

```bash
git rm src/llm/queue.rs
```

In `src/llm/mod.rs` remove `pub mod queue;`.

- [ ] **Step 3: Build**

Run: `cargo build --all-targets`
Expected: builds. If a caller still references `LlmQueue`, fix it now (it must already have been replaced in Task 24, but stale uses might linger).

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -m "Delete obsolete LlmQueue (replaced by scheduler)"
```

---

### Task 30: Plan-1 self-review and final verification

The end-of-plan acceptance check.

- [ ] **Step 1: Run the full test suite**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: zero failures, zero clippy warnings.

- [ ] **Step 2: Manual smoke test**

Build the binaries:

```bash
cargo build --release
```

Run the daemon against a test directory:

```bash
mkdir -p /tmp/clepho-smoke && cp tests/fixtures/photos/*.jpg /tmp/clepho-smoke/
CLEPHO_CONFIG=/tmp/clepho-config.toml ./target/release/clepho-daemon --reset-db &
DAEMON_PID=$!
sleep 30
kill $DAEMON_PID
```

Open the SQLite DB and inspect:

```bash
sqlite3 ~/.local/state/clepho/db.sqlite \
  "SELECT path, scan_done_at, exif_done_at, llm_done_at FROM photos;"
```

Expected: rows present, stage timestamps populated for the stages whose backends were reachable (LLM may be unreachable in smoke; that's fine — `llm_done_at` NULL with `llm_error` set is the correct outcome).

- [ ] **Step 3: Open the TUI**

```bash
./target/release/clepho /tmp/clepho-smoke
```

Verify:
- Folder browser shows files.
- `T` opens Pipeline Status; the smoke folder is listed (if it was registered) or the failures inbox shows LLM errors if applicable.
- `M` toggles managed status (re-press to verify both states).
- `R` triggers a run (status bar message appears).
- `Shift+R` opens the reprocess dialog with a non-zero photo count.

- [ ] **Step 4: Self-review checklist**

Open `docs/superpowers/specs/2026-05-04-pipeline-tagging-alignment-design.md` next to this plan and check:

- [ ] §3 Data Model — every table specified is in `schema_v2.rs` (Task 3).
- [ ] §4 Pipeline — 6-stage DAG (faces is registered as no-op or absent; rest implemented).
- [ ] §4.4 Force-reprocess — Task 23 dialog + apply_reset.
- [ ] §6 Editing/Provenance — Task 5 contract + Task 6 tests (16 cells).
- [ ] §8 Errors — Task 7 (DB events) + Task 8 (JSONL) + Task 22 (UI inbox).
- [ ] §10 Build sequence — Plan 1 covers Phases 1+2 only; faces (Phase 3) deferred to Plan 2.

Discrepancies to fix in this plan rather than defer:
- If a spec section names a feature with no corresponding task, add the task here.
- If a task references types/functions not defined elsewhere in this plan, fix the reference.

- [ ] **Step 5: Final commit + PR**

```bash
git add -A
git commit -m "Plan 1 complete: foundation + 5-stage pipeline"
```

Create a draft PR:

```bash
gh pr create --draft --title "Plan 1: foundation + pipeline (replaces tagging schema, adds scheduler)" \
  --body "$(cat <<'EOF'
## Summary
- Replaces `photos.tags` JSON + parallel `user_tags` with typed facet tables
- Adds per-photo state machine (6 stage timestamp columns) and scheduler with bounded worker pools per stage
- Provenance contract (`source` + `confirmed_at`) enforced at DB layer with table-driven tests for every cell of spec §6.2
- New `pipeline_events` table + JSONL log file for observability
- `R`/`Shift+R`/`M` keys for ad-hoc run / force-reprocess / manage-folder
- Reset-DB dialog detects legacy schema; daemon refuses to start without `--reset-db` flag
- Faces stage stubbed (Plan 2)

## Test plan
- [x] `cargo test` passes
- [x] `cargo clippy -- -D warnings` clean
- [x] Manual: daemon scans a test folder; `clepho` TUI shows status badges; `T` opens Pipeline Status
- [ ] Manual: real LM Studio reachable → LLM stage writes description + objects with provenance
- [ ] Manual: kill daemon mid-stage → restart → completes without duplicates

## Plan
docs/superpowers/plans/2026-05-04-foundation-pipeline.md
EOF
)"
```

---

## Summary

**Total tasks:** 30. Estimated effort: L (large, single-developer sprint). Each task ends with a green commit; tests gate progression.

**Critical path:** Tasks 3 → 4 → 5 → 6 (schema + provenance + tests) before any stage work. Tasks 7-12 (infrastructure) before Tasks 13-17 (stages). Tasks 13-17 in any order (each is independent given the trait). Tasks 18-26 (config, dialogs, daemon) layer on top.

**Subagent-driven execution recommended** — each task is self-contained, has its own commit, and is reviewable in isolation.


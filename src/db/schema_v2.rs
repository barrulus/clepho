//! Fresh schema for clepho v2. The migration story is "drop + recreate" because
//! existing data is experimental (see spec §10.1). Faces tables are defined here
//! but unused in Plan 1 (Plan 2 wires the FaceEngine).
//!
//! See: docs/superpowers/specs/2026-05-04-pipeline-tagging-alignment-design.md §3

#[allow(dead_code)]
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

#[allow(dead_code)]
pub const SCHEMA_GENERATION: i64 = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn schema_v2_applies_cleanly_to_fresh_db() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(SCHEMA_V2)
            .expect("SCHEMA_V2 must be valid SQL");

        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("schema_version row should exist");
        assert_eq!(version, SCHEMA_GENERATION);
    }

    #[test]
    fn schema_v2_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(SCHEMA_V2).expect("first apply");
        conn.execute_batch(SCHEMA_V2)
            .expect("second apply must be a no-op");
    }
}

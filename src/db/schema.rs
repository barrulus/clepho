pub const SCHEMA: &str = r#"
-- Photos table: core photo metadata
CREATE TABLE IF NOT EXISTS photos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    directory TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT,
    modified_at TEXT,
    scanned_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Image metadata
    width INTEGER,
    height INTEGER,
    format TEXT,

    -- EXIF data
    camera_make TEXT,
    camera_model TEXT,
    lens TEXT,
    focal_length REAL,
    aperture REAL,
    shutter_speed TEXT,
    iso INTEGER,
    taken_at TEXT,
    gps_latitude REAL,
    gps_longitude REAL,

    -- Hashes for duplicate detection
    md5_hash TEXT,
    sha256_hash TEXT,
    perceptual_hash TEXT,

    -- LLM-generated content
    description TEXT,
    tags TEXT,  -- JSON array
    llm_processed_at TEXT,

    -- User actions
    marked_for_deletion INTEGER DEFAULT 0,
    is_favorite INTEGER DEFAULT 0
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_photos_directory ON photos(directory);
CREATE INDEX IF NOT EXISTS idx_photos_sha256 ON photos(sha256_hash);
CREATE INDEX IF NOT EXISTS idx_photos_perceptual ON photos(perceptual_hash);
CREATE INDEX IF NOT EXISTS idx_photos_taken_at ON photos(taken_at);
CREATE INDEX IF NOT EXISTS idx_photos_marked_deletion ON photos(marked_for_deletion);

-- Similarity groups: clusters of similar photos
CREATE TABLE IF NOT EXISTS similarity_groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    group_type TEXT NOT NULL,  -- 'exact' or 'perceptual'
    representative_photo_id INTEGER,
    FOREIGN KEY (representative_photo_id) REFERENCES photos(id)
);

-- Photo to similarity group mapping
CREATE TABLE IF NOT EXISTS photo_similarity (
    photo_id INTEGER NOT NULL,
    group_id INTEGER NOT NULL,
    similarity_score REAL,  -- For perceptual matches, the hamming distance
    is_representative INTEGER DEFAULT 0,
    PRIMARY KEY (photo_id, group_id),
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (group_id) REFERENCES similarity_groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_photo_similarity_group ON photo_similarity(group_id);

-- Scan history
CREATE TABLE IF NOT EXISTS scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    directory TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    photos_found INTEGER DEFAULT 0,
    photos_new INTEGER DEFAULT 0,
    photos_updated INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running'  -- 'running', 'completed', 'failed'
);

-- LLM processing queue
CREATE TABLE IF NOT EXISTS llm_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id INTEGER NOT NULL UNIQUE,
    status TEXT DEFAULT 'pending',  -- 'pending', 'processing', 'completed', 'failed'
    queued_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_llm_queue_status ON llm_queue(status);
"#;

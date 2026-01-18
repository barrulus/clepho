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

    -- Complete EXIF data as JSON
    all_exif TEXT,

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
    is_favorite INTEGER DEFAULT 0,

    -- Trash tracking
    original_path TEXT,      -- Path before moving to trash
    trashed_at TEXT          -- ISO timestamp when trashed
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

-- CLIP/Vision embeddings for semantic search
CREATE TABLE IF NOT EXISTS embeddings (
    photo_id INTEGER PRIMARY KEY,
    embedding BLOB NOT NULL,  -- float32 array stored as bytes
    embedding_dim INTEGER NOT NULL,  -- 512, 768, or 1024 depending on model
    model_name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model_name);

-- People: named individuals for face grouping
CREATE TABLE IF NOT EXISTS people (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_people_name ON people(name);

-- Faces: detected faces in photos with bounding boxes and embeddings
CREATE TABLE IF NOT EXISTS faces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id INTEGER NOT NULL,
    bbox_x INTEGER NOT NULL,  -- Bounding box x coordinate
    bbox_y INTEGER NOT NULL,  -- Bounding box y coordinate
    bbox_w INTEGER NOT NULL,  -- Bounding box width
    bbox_h INTEGER NOT NULL,  -- Bounding box height
    embedding BLOB,           -- Face embedding for similarity matching
    embedding_dim INTEGER,    -- Embedding dimension
    person_id INTEGER,        -- NULL until assigned to a person
    confidence REAL,          -- Detection confidence (0-1)
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES people(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);

-- Face clusters: temporary groupings before user names them
CREATE TABLE IF NOT EXISTS face_clusters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    representative_face_id INTEGER,  -- The "best" face in this cluster
    auto_name TEXT,                  -- Auto-generated name like "Person 1"
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (representative_face_id) REFERENCES faces(id) ON DELETE SET NULL
);

-- Face to cluster mapping (many faces can belong to one cluster)
CREATE TABLE IF NOT EXISTS face_cluster_members (
    face_id INTEGER NOT NULL,
    cluster_id INTEGER NOT NULL,
    similarity_score REAL,  -- How similar to cluster representative
    PRIMARY KEY (face_id, cluster_id),
    FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_face_cluster_members_cluster ON face_cluster_members(cluster_id);

-- Track which photos have been scanned for faces (even if 0 faces found)
CREATE TABLE IF NOT EXISTS face_scans (
    photo_id INTEGER PRIMARY KEY,
    scanned_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    faces_found INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

-- Scheduled tasks for automated processing
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type TEXT NOT NULL,           -- 'Scan', 'LlmBatch', 'FaceDetection'
    target_path TEXT NOT NULL,         -- Directory or file path
    photo_ids TEXT,                    -- JSON array of photo IDs for batch operations
    scheduled_at TEXT NOT NULL,        -- ISO timestamp when task should run
    hours_start INTEGER,               -- Optional hour of day to start (0-23)
    hours_end INTEGER,                 -- Optional hour of day to end (0-23)
    status TEXT DEFAULT 'pending',     -- pending/running/completed/cancelled/failed
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_status ON scheduled_tasks(status);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_scheduled_at ON scheduled_tasks(scheduled_at);
"#;

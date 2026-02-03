pub const POSTGRES_SCHEMA: &str = r#"
-- PostgreSQL schema for Clepho

CREATE TABLE IF NOT EXISTS photos (
    id BIGSERIAL PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    directory TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_at TEXT,
    modified_at TEXT,
    scanned_at TEXT NOT NULL DEFAULT NOW(),

    width INTEGER,
    height INTEGER,
    format TEXT,

    camera_make TEXT,
    camera_model TEXT,
    lens TEXT,
    focal_length DOUBLE PRECISION,
    aperture DOUBLE PRECISION,
    shutter_speed TEXT,
    iso INTEGER,
    taken_at TEXT,
    gps_latitude DOUBLE PRECISION,
    gps_longitude DOUBLE PRECISION,
    exif_orientation INTEGER DEFAULT 1,
    user_rotation INTEGER DEFAULT 0,

    all_exif TEXT,

    md5_hash TEXT,
    sha256_hash TEXT,
    perceptual_hash TEXT,

    description TEXT,
    tags TEXT,
    llm_processed_at TEXT,

    marked_for_deletion BOOLEAN DEFAULT FALSE,
    is_favorite BOOLEAN DEFAULT FALSE,

    original_path TEXT,
    trashed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_photos_directory ON photos(directory);
CREATE INDEX IF NOT EXISTS idx_photos_sha256 ON photos(sha256_hash);
CREATE INDEX IF NOT EXISTS idx_photos_perceptual ON photos(perceptual_hash);
CREATE INDEX IF NOT EXISTS idx_photos_taken_at ON photos(taken_at);
CREATE INDEX IF NOT EXISTS idx_photos_marked_deletion ON photos(marked_for_deletion);

CREATE TABLE IF NOT EXISTS similarity_groups (
    id BIGSERIAL PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT NOW(),
    group_type TEXT NOT NULL,
    representative_photo_id BIGINT,
    FOREIGN KEY (representative_photo_id) REFERENCES photos(id)
);

CREATE TABLE IF NOT EXISTS photo_similarity (
    photo_id BIGINT NOT NULL,
    group_id BIGINT NOT NULL,
    similarity_score DOUBLE PRECISION,
    is_representative BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (photo_id, group_id),
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (group_id) REFERENCES similarity_groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_photo_similarity_group ON photo_similarity(group_id);

CREATE TABLE IF NOT EXISTS scans (
    id BIGSERIAL PRIMARY KEY,
    directory TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT NOW(),
    completed_at TEXT,
    photos_found INTEGER DEFAULT 0,
    photos_new INTEGER DEFAULT 0,
    photos_updated INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running'
);

CREATE TABLE IF NOT EXISTS llm_queue (
    id BIGSERIAL PRIMARY KEY,
    photo_id BIGINT NOT NULL UNIQUE,
    status TEXT DEFAULT 'pending',
    queued_at TEXT NOT NULL DEFAULT NOW(),
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_llm_queue_status ON llm_queue(status);

CREATE TABLE IF NOT EXISTS embeddings (
    photo_id BIGINT PRIMARY KEY,
    embedding BYTEA NOT NULL,
    embedding_dim INTEGER NOT NULL,
    model_name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT NOW(),
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model_name);

CREATE TABLE IF NOT EXISTS people (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT NOW(),
    updated_at TEXT NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_people_name ON people(name);

CREATE TABLE IF NOT EXISTS faces (
    id BIGSERIAL PRIMARY KEY,
    photo_id BIGINT NOT NULL,
    bbox_x INTEGER NOT NULL,
    bbox_y INTEGER NOT NULL,
    bbox_w INTEGER NOT NULL,
    bbox_h INTEGER NOT NULL,
    embedding BYTEA,
    embedding_dim INTEGER,
    person_id BIGINT,
    confidence DOUBLE PRECISION,
    created_at TEXT NOT NULL DEFAULT NOW(),
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES people(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);

CREATE TABLE IF NOT EXISTS face_clusters (
    id BIGSERIAL PRIMARY KEY,
    representative_face_id BIGINT,
    auto_name TEXT,
    created_at TEXT NOT NULL DEFAULT NOW(),
    FOREIGN KEY (representative_face_id) REFERENCES faces(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS face_cluster_members (
    face_id BIGINT NOT NULL,
    cluster_id BIGINT NOT NULL,
    similarity_score DOUBLE PRECISION,
    PRIMARY KEY (face_id, cluster_id),
    FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_face_cluster_members_cluster ON face_cluster_members(cluster_id);

CREATE TABLE IF NOT EXISTS face_scans (
    photo_id BIGINT PRIMARY KEY,
    scanned_at TEXT NOT NULL DEFAULT NOW(),
    faces_found INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id BIGSERIAL PRIMARY KEY,
    task_type TEXT NOT NULL,
    target_path TEXT NOT NULL,
    photo_ids TEXT,
    scheduled_at TEXT NOT NULL,
    hours_start INTEGER,
    hours_end INTEGER,
    status TEXT DEFAULT 'pending',
    created_at TEXT DEFAULT NOW(),
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_status ON scheduled_tasks(status);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_scheduled_at ON scheduled_tasks(scheduled_at);

CREATE TABLE IF NOT EXISTS user_tags (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT DEFAULT '#808080',
    created_at TEXT NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_tags_name ON user_tags(name);

CREATE TABLE IF NOT EXISTS photo_user_tags (
    photo_id BIGINT NOT NULL,
    tag_id BIGINT NOT NULL,
    created_at TEXT NOT NULL DEFAULT NOW(),
    PRIMARY KEY (photo_id, tag_id),
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES user_tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_photo_user_tags_tag ON photo_user_tags(tag_id);

CREATE TABLE IF NOT EXISTS albums (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    cover_photo_id BIGINT,
    is_smart BOOLEAN DEFAULT FALSE,
    filter_tags TEXT,
    created_at TEXT NOT NULL DEFAULT NOW(),
    updated_at TEXT NOT NULL DEFAULT NOW(),
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_albums_name ON albums(name);

CREATE TABLE IF NOT EXISTS album_photos (
    album_id BIGINT NOT NULL,
    photo_id BIGINT NOT NULL,
    position INTEGER DEFAULT 0,
    added_at TEXT NOT NULL DEFAULT NOW(),
    PRIMARY KEY (album_id, photo_id),
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
"#;

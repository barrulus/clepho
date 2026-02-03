# Database

Clepho stores all photo metadata, AI descriptions, face data, and scheduled tasks in a database. Two backends are supported:

- **SQLite** (default) - Single-file, zero-configuration
- **PostgreSQL** (optional) - Multi-user, network-accessible, requires the `postgres` feature flag

## Overview

The database provides:

1. **Persistent storage** - Data survives between sessions
2. **Fast queries** - Indexed for quick lookups
3. **ACID compliance** - Reliable transactions
4. **Backend flexibility** - Choose SQLite for simplicity or PostgreSQL for scale

## Backend Selection

Configure in `config.toml`:

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
```

### SQLite (default)

- Single file at `~/.local/share/clepho/clepho.db`
- No setup required
- Good for single-user, local use

### PostgreSQL

- Requires building with `cargo build --features postgres`
- Connection pooling via r2d2 (configurable pool size)
- Better for multi-machine setups or large collections
- See [Migrating to PostgreSQL](#migrating-to-postgresql) below

## Schema Overview

```
┌─────────────────┐     ┌─────────────────┐
│     photos      │────<│   embeddings    │
└─────────────────┘     └─────────────────┘
        │
        │     ┌─────────────────┐
        └────<│     faces       │
              └─────────────────┘
                      │
                      v
              ┌─────────────────┐
              │    people       │
              └─────────────────┘

┌─────────────────┐
│ scheduled_tasks │  (standalone)
└─────────────────┘
```

## Tables

### photos

Main table storing photo metadata.

```sql
CREATE TABLE photos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- File information
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    directory TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,

    -- Timestamps
    created_at TEXT,
    modified_at TEXT,
    scanned_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Image properties
    width INTEGER,
    height INTEGER,
    format TEXT,

    -- Camera/EXIF data
    camera_make TEXT,
    camera_model TEXT,
    lens TEXT,
    focal_length REAL,
    aperture REAL,
    shutter_speed TEXT,
    iso INTEGER,
    taken_at TEXT,

    -- GPS
    gps_latitude REAL,
    gps_longitude REAL,

    -- Complete EXIF as JSON
    all_exif TEXT,

    -- Hash values
    md5_hash TEXT,
    sha256_hash TEXT,
    perceptual_hash TEXT,

    -- AI description
    description TEXT,

    -- Duplicate management
    marked_for_deletion INTEGER DEFAULT 0,

    -- Favorites
    is_favorite INTEGER DEFAULT 0,

    -- Trash tracking
    original_path TEXT,
    trashed_at TEXT
);
```

#### Indexes

```sql
CREATE INDEX idx_photos_directory ON photos(directory);
CREATE INDEX idx_photos_sha256 ON photos(sha256_hash);
CREATE INDEX idx_photos_perceptual ON photos(perceptual_hash);
CREATE INDEX idx_photos_taken_at ON photos(taken_at);
CREATE INDEX idx_photos_marked_deletion ON photos(marked_for_deletion);
```

### embeddings

Stores vector embeddings for semantic search.

```sql
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id INTEGER NOT NULL UNIQUE,
    embedding BLOB NOT NULL,
    model_name TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);
```

### faces

Detected faces in photos.

```sql
CREATE TABLE faces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id INTEGER NOT NULL,

    -- Bounding box
    bbox_x INTEGER NOT NULL,
    bbox_y INTEGER NOT NULL,
    bbox_w INTEGER NOT NULL,
    bbox_h INTEGER NOT NULL,

    -- Face embedding vector
    embedding BLOB,

    -- Link to identified person
    person_id INTEGER,

    -- Detection confidence
    confidence REAL,

    created_at TEXT DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES people(id) ON DELETE SET NULL
);
```

### people

Named individuals identified from faces.

```sql
CREATE TABLE people (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);
```

### face_clusters

Automatic face groupings before naming.

```sql
CREATE TABLE face_clusters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE face_cluster_members (
    cluster_id INTEGER NOT NULL,
    face_id INTEGER NOT NULL,
    PRIMARY KEY (cluster_id, face_id),
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE
);
```

### face_scans

Tracks which photos have been processed for faces.

```sql
CREATE TABLE face_scans (
    photo_id INTEGER PRIMARY KEY,
    scanned_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    faces_found INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);
```

### scheduled_tasks

Scheduled task queue.

```sql
CREATE TABLE scheduled_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Task definition
    task_type TEXT NOT NULL,      -- 'Scan', 'LlmBatch', 'FaceDetection'
    target_path TEXT NOT NULL,
    photo_ids TEXT,               -- JSON array

    -- Scheduling
    scheduled_at TEXT NOT NULL,
    hours_start INTEGER,
    hours_end INTEGER,

    -- Status tracking
    status TEXT DEFAULT 'pending',
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
);

CREATE INDEX idx_scheduled_tasks_status ON scheduled_tasks(status);
CREATE INDEX idx_scheduled_tasks_scheduled_at ON scheduled_tasks(scheduled_at);
```

## Data Types

### Text Fields

- `TEXT` - UTF-8 strings
- Timestamps stored as ISO 8601: `2024-01-15T14:32:00`
- JSON stored as text blobs

### Numeric Fields

- `INTEGER` - 64-bit signed integer
- `REAL` - 64-bit floating point

### Binary Fields

- `BLOB` - Binary data (embeddings stored as float arrays)

## Common Queries

### Find Photos in Directory

```sql
SELECT * FROM photos
WHERE directory = '/home/user/Photos/2024'
ORDER BY filename;
```

### Find Exact Duplicates

```sql
SELECT sha256_hash, COUNT(*) as count, GROUP_CONCAT(path) as paths
FROM photos
WHERE sha256_hash IS NOT NULL
GROUP BY sha256_hash
HAVING count > 1;
```

### Find Photos by Person

```sql
SELECT p.* FROM photos p
JOIN faces f ON f.photo_id = p.id
JOIN people pe ON f.person_id = pe.id
WHERE pe.name = 'John Smith';
```

### Get Pending Schedules

```sql
SELECT * FROM scheduled_tasks
WHERE status = 'pending'
AND scheduled_at <= datetime('now')
ORDER BY scheduled_at;
```

### Search Descriptions

```sql
SELECT * FROM photos
WHERE description LIKE '%beach%sunset%'
ORDER BY taken_at DESC;
```

## Maintenance

### Backup

**SQLite:**
```bash
# Simple copy
cp ~/.local/share/clepho/clepho.db ~/backup/clepho_backup.db

# While Clepho is running (SQLite handles this safely)
sqlite3 ~/.local/share/clepho/clepho.db ".backup ~/backup/clepho_backup.db"
```

**PostgreSQL:**
```bash
pg_dump clepho > ~/backup/clepho_backup.sql
```

### Vacuum

Reclaim space after deletions:

**SQLite:**
```bash
sqlite3 ~/.local/share/clepho/clepho.db "VACUUM;"
```

**PostgreSQL:**
```bash
psql -d clepho -c "VACUUM ANALYZE;"
```

### Integrity Check

**SQLite:**
```bash
sqlite3 ~/.local/share/clepho/clepho.db "PRAGMA integrity_check;"
```

### Size Check

**SQLite:**
```bash
ls -lh ~/.local/share/clepho/clepho.db
```

**PostgreSQL:**
```bash
psql -d clepho -c "SELECT pg_size_pretty(pg_database_size('clepho'));"
```

## Direct Access

### SQLite CLI

```bash
sqlite3 ~/.local/share/clepho/clepho.db

# Useful commands
.tables          -- List all tables
.schema photos   -- Show table schema
.headers on      -- Show column headers
.mode column     -- Column output format
```

### PostgreSQL CLI

```bash
psql "postgresql://user:password@localhost:5432/clepho"

# Useful commands
\dt              -- List all tables
\d photos        -- Show table schema
```

### Example Queries

```sql
-- Count photos
SELECT COUNT(*) FROM photos;

-- Photos by camera
SELECT camera_model, COUNT(*)
FROM photos
GROUP BY camera_model
ORDER BY COUNT(*) DESC;

-- Storage by directory
SELECT directory, SUM(size_bytes)/1024/1024 as mb
FROM photos
GROUP BY directory
ORDER BY mb DESC
LIMIT 10;

-- Recent scans
SELECT filename, scanned_at
FROM photos
ORDER BY scanned_at DESC
LIMIT 20;
```

## Migrating to PostgreSQL

Clepho includes a built-in migration tool that copies all data from SQLite to PostgreSQL, preserving IDs and relationships.

### Prerequisites

1. Build with PostgreSQL support: `cargo build --release --features postgres`
2. Create a PostgreSQL database: `createdb clepho`
3. Have access to your existing SQLite database

### Running the Migration

```bash
# Migrate using the default config (reads sqlite_path from config.toml)
clepho --migrate-to-postgres "postgresql://user:password@localhost:5432/clepho"

# Or specify a custom config file
clepho -c /path/to/config.toml --migrate-to-postgres "postgresql://user:password@localhost:5432/clepho"
```

The migration:
- Creates the PostgreSQL schema (tables and indexes)
- Copies all 16 tables in foreign-key-safe order
- Preserves original row IDs
- Resets PostgreSQL sequences for correct auto-increment
- Uses `ON CONFLICT DO NOTHING`, so it's safe to re-run

### After Migration

Update your config to use PostgreSQL:

```toml
[database]
backend = "postgresql"
postgresql_url = "postgresql://user:password@localhost:5432/clepho"
pool_size = 10
```

## Schema Updates

Clepho automatically migrates the schema on startup:
- New columns added with defaults
- New tables created
- Indexes added

### Manual Migration (SQLite)

If needed, you can add columns manually:

```sql
-- Example: Add a new column
ALTER TABLE photos ADD COLUMN rating INTEGER DEFAULT 0;
```

## Performance

### Index Usage

Queries use indexes when:
- Filtering by `directory`
- Matching `sha256_hash` or `perceptual_hash`
- Filtering by `taken_at`
- Checking `marked_for_deletion`

### Query Optimization

```sql
-- Use EXPLAIN to check query plan
EXPLAIN QUERY PLAN
SELECT * FROM photos WHERE directory = '/path';
```

### Database Size

Typical sizes:
- ~1KB per photo (metadata only)
- +1.5KB per photo with embedding
- +0.5KB per face

| Photos | Approx Size |
|--------|-------------|
| 1,000 | ~2 MB |
| 10,000 | ~20 MB |
| 100,000 | ~200 MB |

## Troubleshooting

### Database Locked (SQLite)

```
Error: database is locked
```

**Solutions:**
- Close other Clepho instances
- Close SQLite CLI sessions
- Check for zombie processes

### Connection Refused (PostgreSQL)

```
Error: Failed to connect to PostgreSQL
```

**Solutions:**
- Verify PostgreSQL is running: `pg_isready`
- Check the connection URL in config.toml
- Ensure the database exists: `createdb clepho`
- Check firewall/pg_hba.conf for access

### Corrupt Database (SQLite)

```
Error: database disk image is malformed
```

**Recovery:**
1. Try integrity check
2. Export what's readable
3. Restore from backup

```bash
# Attempt recovery
sqlite3 corrupt.db ".dump" | sqlite3 new.db
```

### Slow Queries

- Check indexes exist
- Run VACUUM (SQLite) or VACUUM ANALYZE (PostgreSQL)
- For PostgreSQL, consider increasing `pool_size` if many concurrent operations

### Missing Data

- Verify file was scanned
- Check scan completed without errors
- Re-scan if necessary

## Data Privacy

### Local Only

- All data stored locally
- No cloud sync built-in
- No telemetry

### Sensitive Data

The database may contain:
- File paths (reveal directory structure)
- GPS coordinates (reveal locations)
- Face data (biometric information)
- AI descriptions (content analysis)

### Secure Deletion

To completely remove data:
1. Delete specific records
2. Run VACUUM
3. Or delete entire database file

```sql
-- Delete a person's data
DELETE FROM faces WHERE person_id = ?;
DELETE FROM people WHERE id = ?;
VACUUM;
```

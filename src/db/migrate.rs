//! SQLite-to-PostgreSQL migration tool.
//!
//! Reads all tables from the SQLite database and bulk-inserts into PostgreSQL,
//! respecting foreign key ordering. Preserves original IDs and resets
//! PostgreSQL sequences afterward.

use anyhow::{Context, Result};
use postgres::NoTls;
use rusqlite::Connection;

use super::postgres_schema::POSTGRES_SCHEMA;

/// Migrate all data from a SQLite database to a PostgreSQL database.
///
/// The PostgreSQL schema is created first (tables + indexes), then data is
/// copied table-by-table in foreign-key-safe order. Original IDs are preserved
/// and PostgreSQL sequences are reset to continue from max(id)+1.
pub fn migrate_sqlite_to_postgres(sqlite_path: &std::path::Path, postgres_url: &str) -> Result<()> {
    // Open SQLite source
    let sqlite = Connection::open(sqlite_path)
        .with_context(|| format!("Failed to open SQLite database: {}", sqlite_path.display()))?;

    // Connect to PostgreSQL
    let mut pg = postgres::Client::connect(postgres_url, NoTls)
        .with_context(|| "Failed to connect to PostgreSQL")?;

    // Create schema
    eprintln!("Creating PostgreSQL schema...");
    pg.batch_execute(POSTGRES_SCHEMA)
        .with_context(|| "Failed to create PostgreSQL schema")?;

    // Migrate tables in foreign-key-safe order
    migrate_photos(&sqlite, &mut pg)?;
    migrate_people(&sqlite, &mut pg)?;
    migrate_faces(&sqlite, &mut pg)?;
    migrate_face_scans(&sqlite, &mut pg)?;
    migrate_embeddings(&sqlite, &mut pg)?;
    migrate_face_clusters(&sqlite, &mut pg)?;
    migrate_face_cluster_members(&sqlite, &mut pg)?;
    migrate_similarity_groups(&sqlite, &mut pg)?;
    migrate_photo_similarity(&sqlite, &mut pg)?;
    migrate_scans(&sqlite, &mut pg)?;
    migrate_llm_queue(&sqlite, &mut pg)?;
    migrate_user_tags(&sqlite, &mut pg)?;
    migrate_photo_user_tags(&sqlite, &mut pg)?;
    migrate_albums(&sqlite, &mut pg)?;
    migrate_album_photos(&sqlite, &mut pg)?;
    migrate_scheduled_tasks(&sqlite, &mut pg)?;

    // Reset all sequences to max(id) + 1
    reset_sequences(&mut pg)?;

    eprintln!("Migration complete!");
    Ok(())
}

fn migrate_photos(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, path, filename, directory, size_bytes, created_at, modified_at, scanned_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso,
                taken_at, gps_latitude, gps_longitude, exif_orientation, user_rotation,
                all_exif, md5_hash, sha256_hash, perceptual_hash,
                description, tags, llm_processed_at,
                marked_for_deletion, is_favorite,
                original_path, trashed_at
         FROM photos"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<i64>>(8)?,
            row.get::<_, Option<i64>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<String>>(13)?,
            row.get::<_, Option<f64>>(14)?,
            row.get::<_, Option<f64>>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<i64>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<f64>>(19)?,
            row.get::<_, Option<f64>>(20)?,
            row.get::<_, Option<i32>>(21)?,
            row.get::<_, Option<i32>>(22)?,
            row.get::<_, Option<String>>(23)?,
            row.get::<_, Option<String>>(24)?,
            row.get::<_, Option<String>>(25)?,
            row.get::<_, Option<String>>(26)?,
            row.get::<_, Option<String>>(27)?,
            row.get::<_, Option<String>>(28)?,
            row.get::<_, Option<String>>(29)?,
            row.get::<_, i64>(30)?,
            row.get::<_, i64>(31)?,
            row.get::<_, Option<String>>(32)?,
            row.get::<_, Option<String>>(33)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO photos (id, path, filename, directory, size_bytes, created_at, modified_at, scanned_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso,
                taken_at, gps_latitude, gps_longitude, exif_orientation, user_rotation,
                all_exif, md5_hash, sha256_hash, perceptual_hash,
                description, tags, llm_processed_at,
                marked_for_deletion, is_favorite,
                original_path, trashed_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34)
             ON CONFLICT (id) DO NOTHING",
            &[
                &r.0, &r.1, &r.2, &r.3, &r.4, &r.5, &r.6, &r.7,
                &(r.8.map(|v| v as i32)), &(r.9.map(|v| v as i32)), &r.10,
                &r.11, &r.12, &r.13, &r.14, &r.15, &r.16, &(r.17.map(|v| v as i32)),
                &r.18, &r.19, &r.20, &r.21.unwrap_or(1), &r.22.unwrap_or(0),
                &r.23, &r.24, &r.25, &r.26,
                &r.27, &r.28, &r.29,
                &(r.30 != 0), &(r.31 != 0),
                &r.32, &r.33,
            ],
        )?;
        count += 1;
    }
    eprintln!("  photos: {} rows migrated", count);
    Ok(())
}

fn migrate_people(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare("SELECT id, name, created_at, updated_at FROM people")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO people (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3],
        )?;
        count += 1;
    }
    eprintln!("  people: {} rows migrated", count);
    Ok(())
}

fn migrate_faces(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, embedding_dim,
                person_id, confidence, created_at
         FROM faces"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, i32>(5)?,
            row.get::<_, Option<Vec<u8>>>(6)?,
            row.get::<_, Option<i32>>(7)?,
            row.get::<_, Option<i64>>(8)?,
            row.get::<_, Option<f64>>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO faces (id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, embedding_dim,
                person_id, confidence, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &r.4, &r.5, &r.6, &r.7, &r.8, &r.9, &r.10],
        )?;
        count += 1;
    }
    eprintln!("  faces: {} rows migrated", count);
    Ok(())
}

fn migrate_face_scans(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare("SELECT photo_id, scanned_at, faces_found FROM face_scans")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO face_scans (photo_id, scanned_at, faces_found) VALUES ($1, $2, $3)
             ON CONFLICT (photo_id) DO NOTHING",
            &[&r.0, &r.1, &r.2],
        )?;
        count += 1;
    }
    eprintln!("  face_scans: {} rows migrated", count);
    Ok(())
}

fn migrate_embeddings(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT photo_id, embedding, embedding_dim, model_name, created_at FROM embeddings"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO embeddings (photo_id, embedding, embedding_dim, model_name, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (photo_id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &r.4],
        )?;
        count += 1;
    }
    eprintln!("  embeddings: {} rows migrated", count);
    Ok(())
}

fn migrate_face_clusters(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, representative_face_id, auto_name, created_at FROM face_clusters"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO face_clusters (id, representative_face_id, auto_name, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3],
        )?;
        count += 1;
    }
    eprintln!("  face_clusters: {} rows migrated", count);
    Ok(())
}

fn migrate_face_cluster_members(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT face_id, cluster_id, similarity_score FROM face_cluster_members"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<f64>>(2)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO face_cluster_members (face_id, cluster_id, similarity_score)
             VALUES ($1, $2, $3)
             ON CONFLICT (face_id, cluster_id) DO NOTHING",
            &[&r.0, &r.1, &r.2],
        )?;
        count += 1;
    }
    eprintln!("  face_cluster_members: {} rows migrated", count);
    Ok(())
}

fn migrate_similarity_groups(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, created_at, group_type, representative_photo_id FROM similarity_groups"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO similarity_groups (id, created_at, group_type, representative_photo_id)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3],
        )?;
        count += 1;
    }
    eprintln!("  similarity_groups: {} rows migrated", count);
    Ok(())
}

fn migrate_photo_similarity(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT photo_id, group_id, similarity_score, is_representative FROM photo_similarity"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO photo_similarity (photo_id, group_id, similarity_score, is_representative)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (photo_id, group_id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &(r.3 != 0)],
        )?;
        count += 1;
    }
    eprintln!("  photo_similarity: {} rows migrated", count);
    Ok(())
}

fn migrate_scans(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, directory, started_at, completed_at, photos_found, photos_new, photos_updated, status
         FROM scans"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i32>>(4)?,
            row.get::<_, Option<i32>>(5)?,
            row.get::<_, Option<i32>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO scans (id, directory, started_at, completed_at, photos_found, photos_new, photos_updated, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &r.4, &r.5, &r.6, &r.7],
        )?;
        count += 1;
    }
    eprintln!("  scans: {} rows migrated", count);
    Ok(())
}

fn migrate_llm_queue(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, photo_id, status, queued_at, started_at, completed_at, error_message FROM llm_queue"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO llm_queue (id, photo_id, status, queued_at, started_at, completed_at, error_message)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &r.4, &r.5, &r.6],
        )?;
        count += 1;
    }
    eprintln!("  llm_queue: {} rows migrated", count);
    Ok(())
}

fn migrate_user_tags(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare("SELECT id, name, color, created_at FROM user_tags")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO user_tags (id, name, color, created_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3],
        )?;
        count += 1;
    }
    eprintln!("  user_tags: {} rows migrated", count);
    Ok(())
}

fn migrate_photo_user_tags(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT photo_id, tag_id, created_at FROM photo_user_tags"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO photo_user_tags (photo_id, tag_id, created_at) VALUES ($1, $2, $3)
             ON CONFLICT (photo_id, tag_id) DO NOTHING",
            &[&r.0, &r.1, &r.2],
        )?;
        count += 1;
    }
    eprintln!("  photo_user_tags: {} rows migrated", count);
    Ok(())
}

fn migrate_albums(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, name, description, cover_photo_id, is_smart, filter_tags, created_at, updated_at
         FROM albums"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO albums (id, name, description, cover_photo_id, is_smart, filter_tags, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &(r.4 != 0), &r.5, &r.6, &r.7],
        )?;
        count += 1;
    }
    eprintln!("  albums: {} rows migrated", count);
    Ok(())
}

fn migrate_album_photos(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT album_id, photo_id, position, added_at FROM album_photos"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<i32>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO album_photos (album_id, photo_id, position, added_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (album_id, photo_id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3],
        )?;
        count += 1;
    }
    eprintln!("  album_photos: {} rows migrated", count);
    Ok(())
}

fn migrate_scheduled_tasks(sqlite: &Connection, pg: &mut postgres::Client) -> Result<()> {
    let mut stmt = sqlite.prepare(
        "SELECT id, task_type, target_path, photo_ids, scheduled_at,
                hours_start, hours_end, status, created_at,
                started_at, completed_at, error_message
         FROM scheduled_tasks"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i32>>(5)?,
            row.get::<_, Option<i32>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
        ))
    })?;

    let mut count = 0u64;
    for row in rows {
        let r = row?;
        pg.execute(
            "INSERT INTO scheduled_tasks (id, task_type, target_path, photo_ids, scheduled_at,
                hours_start, hours_end, status, created_at,
                started_at, completed_at, error_message)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (id) DO NOTHING",
            &[&r.0, &r.1, &r.2, &r.3, &r.4, &r.5, &r.6, &r.7, &r.8, &r.9, &r.10, &r.11],
        )?;
        count += 1;
    }
    eprintln!("  scheduled_tasks: {} rows migrated", count);
    Ok(())
}

/// Reset all BIGSERIAL sequences to max(id) + 1 so new inserts get correct IDs.
fn reset_sequences(pg: &mut postgres::Client) -> Result<()> {
    let sequences = [
        ("photos", "photos_id_seq"),
        ("people", "people_id_seq"),
        ("faces", "faces_id_seq"),
        ("similarity_groups", "similarity_groups_id_seq"),
        ("scans", "scans_id_seq"),
        ("llm_queue", "llm_queue_id_seq"),
        ("face_clusters", "face_clusters_id_seq"),
        ("user_tags", "user_tags_id_seq"),
        ("albums", "albums_id_seq"),
        ("scheduled_tasks", "scheduled_tasks_id_seq"),
    ];

    for (table, seq) in &sequences {
        let row = pg.query_one(
            &format!("SELECT COALESCE(MAX(id), 0) FROM {}", table),
            &[],
        )?;
        let max_id: i64 = row.get(0);
        if max_id > 0 {
            pg.execute(
                &format!("SELECT setval('{}', $1)", seq),
                &[&max_id],
            )?;
        }
    }

    eprintln!("  Sequences reset to current max IDs");
    Ok(())
}

//! Index stage: post-process after model stages. Recomputes smart-album
//! membership for THIS photo. Plan 2 will add person-centroid recompute.

use crate::db::clock::Clock;
use crate::db::filter_eval::{matches_photo, Filter};
use crate::pipeline::stages::{PendingPhoto, Stage, StageId, StageOutcome};
use anyhow::Result;
use rusqlite::{params, Connection};

#[allow(dead_code)]
pub struct IndexStage;

impl Stage for IndexStage {
    fn id(&self) -> StageId {
        StageId::Index
    }

    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>> {
        // Index runs after thumb, llm and (in Plan 2) faces. Conservative:
        // require all the model stages we have in Plan 1.
        let sql = match folder {
            Some(_) => {
                "SELECT id, path FROM photos
                 WHERE index_done_at IS NULL
                   AND exif_done_at IS NOT NULL
                   AND thumb_done_at IS NOT NULL
                   AND llm_done_at IS NOT NULL
                   AND path LIKE ?1
                 ORDER BY id ASC LIMIT ?2"
            }
            None => {
                "SELECT id, path FROM photos
                 WHERE index_done_at IS NULL
                   AND exif_done_at IS NOT NULL
                   AND thumb_done_at IS NOT NULL
                   AND llm_done_at IS NOT NULL
                 ORDER BY id ASC LIMIT ?1"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = match folder {
            Some(f) => {
                let prefix = format!("{}/%", f.trim_end_matches('/'));
                stmt.query_map(params![prefix, limit as i64], |r| {
                    Ok(PendingPhoto {
                        id: r.get(0)?,
                        path: std::path::PathBuf::from(r.get::<_, String>(1)?),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => stmt
                .query_map(params![limit as i64], |r| {
                    Ok(PendingPhoto {
                        id: r.get(0)?,
                        path: std::path::PathBuf::from(r.get::<_, String>(1)?),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(rows)
    }

    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        _clock: &dyn Clock,
    ) -> Result<StageOutcome> {
        let smart: Vec<(i64, String)> = conn
            .prepare(
                "SELECT id, filter_json FROM albums
                 WHERE kind='smart' AND filter_json IS NOT NULL",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let tx = conn.unchecked_transaction()?;
        for (album_id, filter_json) in smart {
            let filter: Filter = match serde_json::from_str(&filter_json) {
                Ok(f) => f,
                Err(_) => continue, // bad filter JSON: skip this album
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

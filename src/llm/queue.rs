use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

use super::client::LlmClient;
use crate::db::Database;

#[derive(Debug, Clone)]
pub struct LlmTask {
    pub photo_id: i64,
    pub photo_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum LlmTaskStatus {
    Queued { total: usize },
    Processing { current: usize, total: usize, path: String },
    Completed { processed: usize, failed: usize },
    Error { message: String },
}

pub struct LlmQueue {
    tasks: VecDeque<LlmTask>,
    client: LlmClient,
}

impl LlmQueue {
    pub fn new(client: LlmClient) -> Self {
        Self {
            tasks: VecDeque::new(),
            client,
        }
    }

    #[allow(dead_code)]
    pub fn add_task(&mut self, task: LlmTask) {
        self.tasks.push_back(task);
    }

    pub fn add_tasks(&mut self, tasks: Vec<LlmTask>) {
        for task in tasks {
            self.tasks.push_back(task);
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn process_all(
        &mut self,
        db: &Database,
        progress_tx: Option<mpsc::Sender<LlmTaskStatus>>,
    ) -> Result<(usize, usize)> {
        let total = self.tasks.len();
        let mut processed = 0;
        let mut failed = 0;

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(LlmTaskStatus::Queued { total });
        }

        while let Some(task) = self.tasks.pop_front() {
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(LlmTaskStatus::Processing {
                    current: processed + 1,
                    total,
                    path: task.photo_path.to_string_lossy().to_string(),
                });
            }

            match self.process_task(&task, db) {
                Ok(_) => processed += 1,
                Err(e) => {
                    failed += 1;
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(LlmTaskStatus::Error {
                            message: format!("Error processing {}: {}", task.photo_path.display(), e),
                        });
                    }
                }
            }
        }

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(LlmTaskStatus::Completed { processed, failed });
        }

        Ok((processed, failed))
    }

    fn process_task(&self, task: &LlmTask, db: &Database) -> Result<()> {
        // Get image description from LLM
        let description = self.client.describe_image(&task.photo_path)?;

        // Generate tags from description
        let tags = self.client.generate_tags(&description)?;
        let tags_json = serde_json::to_string(&tags)?;

        // Update database with description and tags
        db.conn.execute(
            r#"
            UPDATE photos
            SET description = ?, tags = ?, llm_processed_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
            rusqlite::params![description, tags_json, task.photo_id],
        )?;

        Ok(())
    }
}

impl Database {
    #[allow(dead_code)]
    pub fn get_photos_without_description(&self) -> Result<Vec<LlmTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL
            ORDER BY scanned_at DESC
            "#,
        )?;

        let tasks = stmt
            .query_map([], |row| {
                Ok(LlmTask {
                    photo_id: row.get(0)?,
                    photo_path: PathBuf::from(row.get::<_, String>(1)?),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    pub fn get_photos_without_description_in_dir(&self, directory: &std::path::Path) -> Result<Vec<LlmTask>> {
        let dir_str = directory.to_string_lossy();
        let pattern = format!("{}%", dir_str);

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL AND path LIKE ?
            ORDER BY path ASC
            "#,
        )?;

        let tasks = stmt
            .query_map([pattern], |row| {
                Ok(LlmTask {
                    photo_id: row.get(0)?,
                    photo_path: PathBuf::from(row.get::<_, String>(1)?),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    #[allow(dead_code)]
    pub fn get_photo_description(&self, photo_id: i64) -> Result<Option<String>> {
        let result: Option<String> = self.conn.query_row(
            "SELECT description FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        ).ok();

        Ok(result)
    }
}

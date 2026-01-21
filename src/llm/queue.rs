use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use super::client::LlmClient;
use crate::db::Database;
use crate::tasks::{TaskUpdate, TaskProgress};

#[derive(Debug, Clone)]
pub struct LlmTask {
    pub photo_id: i64,
    pub photo_path: PathBuf,
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

    /// Process all tasks with cancellation support via TaskUpdate protocol.
    pub fn process_all_cancellable(
        &mut self,
        db: &Database,
        tx: mpsc::Sender<TaskUpdate>,
        cancel_flag: Arc<AtomicBool>,
    ) {
        let total = self.tasks.len();
        let mut processed = 0;
        let mut failed = 0;

        let _ = tx.send(TaskUpdate::Started { total });

        while let Some(task) = self.tasks.pop_front() {
            // Check for cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            let filename = task.photo_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| task.photo_path.to_string_lossy().to_string());

            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(processed + 1, total).with_item(&filename)
            ));

            match self.process_task(&task, db) {
                Ok(_) => processed += 1,
                Err(e) => {
                    failed += 1;
                    // Continue processing other tasks despite errors
                    tracing::error!(path = %task.photo_path.display(), error = %e, "LLM processing error");
                }
            }
        }

        if failed > 0 {
            let _ = tx.send(TaskUpdate::Completed {
                message: format!("{} processed, {} failed", processed, failed),
            });
        } else {
            let _ = tx.send(TaskUpdate::Completed {
                message: format!("{} photos processed", processed),
            });
        }
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

        // Generate and store embedding for semantic search (if provider supports it)
        if self.client.supports_embeddings() {
            // Use the description for the embedding
            if let Ok(embedding) = self.client.get_text_embedding(&description) {
                // Store with the model name for tracking
                let _ = db.store_embedding(task.photo_id, &embedding, "text-embedding");
            }
        }

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

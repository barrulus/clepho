use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use super::client::LlmClient;
use crate::config::DatabaseConfig;
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

    /// Process all tasks in parallel with configurable concurrency.
    pub fn process_all_parallel(
        &mut self,
        db_config: &DatabaseConfig,
        tx: mpsc::Sender<TaskUpdate>,
        cancel_flag: Arc<AtomicBool>,
        concurrency: usize,
    ) {
        let concurrency = concurrency.max(1);
        let total = self.tasks.len();

        let _ = tx.send(TaskUpdate::Started { total });

        let work_queue: Arc<Mutex<VecDeque<LlmTask>>> =
            Arc::new(Mutex::new(self.tasks.drain(..).collect()));
        let processed = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let consecutive_failures = Arc::new(AtomicUsize::new(0));
        let abort_flag = Arc::new(AtomicBool::new(false));

        const MAX_CONSECUTIVE_FAILURES: usize = 3;

        std::thread::scope(|scope| {
            for _ in 0..concurrency {
                let work_queue = work_queue.clone();
                let client = self.client.clone();
                let db_config = db_config.clone();
                let tx = tx.clone();
                let cancel_flag = cancel_flag.clone();
                let abort_flag = abort_flag.clone();
                let processed = processed.clone();
                let failed = failed.clone();
                let consecutive_failures = consecutive_failures.clone();

                scope.spawn(move || {
                    let db = match Database::open(&db_config) {
                        Ok(db) => db,
                        Err(e) => {
                            tracing::error!(error = %e, "Worker failed to open database");
                            return;
                        }
                    };

                    loop {
                        if cancel_flag.load(Ordering::SeqCst) || abort_flag.load(Ordering::SeqCst) {
                            break;
                        }

                        let task = {
                            let mut queue = work_queue.lock().unwrap();
                            queue.pop_front()
                        };

                        let task = match task {
                            Some(t) => t,
                            None => break, // No more work
                        };

                        let done = processed.load(Ordering::SeqCst) + failed.load(Ordering::SeqCst) + 1;
                        let filename = task.photo_path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| task.photo_path.to_string_lossy().to_string());

                        let _ = tx.send(TaskUpdate::Progress(
                            TaskProgress::new(done, total).with_item(&filename)
                        ));

                        match process_task(&client, &task, &db) {
                            Ok(_) => {
                                processed.fetch_add(1, Ordering::SeqCst);
                                consecutive_failures.store(0, Ordering::SeqCst);
                            }
                            Err(e) => {
                                failed.fetch_add(1, Ordering::SeqCst);
                                let cf = consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;

                                if cf <= MAX_CONSECUTIVE_FAILURES {
                                    tracing::error!(path = %task.photo_path.display(), error = %e, "LLM processing error");
                                }

                                if cf >= MAX_CONSECUTIVE_FAILURES {
                                    tracing::error!(
                                        consecutive_failures = cf,
                                        "Aborting LLM batch: too many consecutive failures (server may be unavailable)"
                                    );
                                    abort_flag.store(true, Ordering::SeqCst);
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });

        let p = processed.load(Ordering::SeqCst);
        let f = failed.load(Ordering::SeqCst);

        if cancel_flag.load(Ordering::SeqCst) {
            let _ = tx.send(TaskUpdate::Cancelled);
        } else if abort_flag.load(Ordering::SeqCst) {
            let _ = tx.send(TaskUpdate::Completed {
                message: format!(
                    "Aborted: LLM server unavailable ({} processed, {} failed)",
                    p, f
                ),
            });
        } else if f > 0 {
            let _ = tx.send(TaskUpdate::Completed {
                message: format!("{} processed, {} failed", p, f),
            });
        } else {
            let _ = tx.send(TaskUpdate::Completed {
                message: format!("{} photos processed", p),
            });
        }
    }

    /// Process all tasks sequentially with cancellation support (legacy).
    #[allow(dead_code)]
    pub fn process_all_cancellable(
        &mut self,
        db: &Database,
        tx: mpsc::Sender<TaskUpdate>,
        cancel_flag: Arc<AtomicBool>,
    ) {
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        let total = self.tasks.len();
        let mut processed = 0;
        let mut failed = 0;
        let mut consecutive_failures = 0;

        let _ = tx.send(TaskUpdate::Started { total });

        while let Some(task) = self.tasks.pop_front() {
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            let filename = task.photo_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| task.photo_path.to_string_lossy().to_string());

            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(processed + failed + 1, total).with_item(&filename)
            ));

            match process_task(&self.client, &task, db) {
                Ok(_) => {
                    processed += 1;
                    consecutive_failures = 0;
                }
                Err(e) => {
                    failed += 1;
                    consecutive_failures += 1;

                    if consecutive_failures <= MAX_CONSECUTIVE_FAILURES {
                        tracing::error!(path = %task.photo_path.display(), error = %e, "LLM processing error");
                    }

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        tracing::error!(
                            consecutive_failures = consecutive_failures,
                            "Aborting LLM batch: too many consecutive failures (server may be unavailable)"
                        );
                        let _ = tx.send(TaskUpdate::Completed {
                            message: format!(
                                "Aborted: LLM server unavailable ({} processed, {} failed)",
                                processed, failed
                            ),
                        });
                        return;
                    }
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
}

fn process_task(client: &LlmClient, task: &LlmTask, db: &Database) -> Result<()> {
    let (description, tags) = client.describe_and_tag_image(&task.photo_path)?;
    let tags_json = serde_json::to_string(&tags)?;

    db.save_llm_result(task.photo_id, &description, &tags_json)?;

    if client.supports_embeddings() {
        if let Ok(embedding) = client.get_text_embedding(&description) {
            let _ = db.store_embedding(task.photo_id, &embedding, "text-embedding");
        }
    }

    Ok(())
}

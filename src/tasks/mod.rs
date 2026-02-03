//! Background task management for non-blocking operations.
//!
//! This module provides a unified system for managing background tasks like
//! scanning, LLM processing, and face detection without blocking the UI.

pub mod manager;

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

pub use manager::BackgroundTaskManager;

/// Unique identifier for a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

impl TaskId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        TaskId(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Scan,
    LlmSingle,
    LlmBatch,
    FaceDetection,
    FaceClustering,
    ClipEmbedding,
    FindDuplicates,
}

impl TaskType {
    /// Short display name for status bar.
    pub fn short_name(&self) -> &'static str {
        match self {
            TaskType::Scan => "S",
            TaskType::LlmSingle => "L",
            TaskType::LlmBatch => "B",
            TaskType::FaceDetection => "F",
            TaskType::FaceClustering => "C",
            TaskType::ClipEmbedding => "E",
            TaskType::FindDuplicates => "D",
        }
    }

    /// Full display name for task list.
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskType::Scan => "Directory Scan",
            TaskType::LlmSingle => "LLM Description",
            TaskType::LlmBatch => "LLM Batch Process",
            TaskType::FaceDetection => "Face Detection",
            TaskType::FaceClustering => "Face Clustering",
            TaskType::ClipEmbedding => "CLIP Embedding",
            TaskType::FindDuplicates => "Find Duplicates",
        }
    }
}

/// Progress information for a task.
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub current: usize,
    pub total: usize,
    pub current_item: Option<String>,
    pub message: Option<String>,
}

impl TaskProgress {
    pub fn new(current: usize, total: usize) -> Self {
        Self {
            current,
            total,
            current_item: None,
            message: None,
        }
    }

    pub fn with_item(mut self, item: impl Into<String>) -> Self {
        self.current_item = Some(item.into());
        self
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    /// Calculate progress percentage (0-100).
    pub fn percent(&self) -> u8 {
        if self.total == 0 {
            0
        } else {
            ((self.current as f64 / self.total as f64) * 100.0).min(100.0) as u8
        }
    }
}

/// State of a background task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Completed,
    Cancelled,
    Failed(String),
}

/// Update messages sent from background tasks via channels.
#[derive(Debug, Clone)]
pub enum TaskUpdate {
    /// Task has started with total items to process.
    Started { total: usize },
    /// Progress update during processing.
    Progress(TaskProgress),
    /// Task completed successfully.
    Completed { message: String },
    /// Task was cancelled.
    Cancelled,
    /// Task failed with error.
    Failed { error: String },
}

/// A running background task with its state and communication channels.
pub struct BackgroundTask {
    pub id: TaskId,
    pub task_type: TaskType,
    pub state: TaskState,
    pub progress: Option<TaskProgress>,
    pub cancel_flag: Arc<AtomicBool>,
    pub receiver: mpsc::Receiver<TaskUpdate>,
    pub started_at: Instant,
}

impl BackgroundTask {
    /// Create a new background task.
    pub fn new(
        task_type: TaskType,
        cancel_flag: Arc<AtomicBool>,
        receiver: mpsc::Receiver<TaskUpdate>,
    ) -> Self {
        Self {
            id: TaskId::new(),
            task_type,
            state: TaskState::Running,
            progress: None,
            cancel_flag,
            receiver,
            started_at: Instant::now(),
        }
    }

    /// Request cancellation of this task.
    pub fn cancel(&self) {
        self.cancel_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get elapsed time since task started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Check if task is still running.
    pub fn is_running(&self) -> bool {
        self.state == TaskState::Running
    }
}

/// Result of polling task updates.
#[derive(Debug, Clone)]
pub struct TaskCompletionInfo {
    pub id: TaskId,
    pub task_type: TaskType,
    pub message: String,
    pub success: bool,
}

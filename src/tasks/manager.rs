//! Background task manager for tracking and controlling concurrent tasks.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use super::{BackgroundTask, TaskCompletionInfo, TaskId, TaskProgress, TaskState, TaskType, TaskUpdate};

/// Manages all background tasks, providing centralized control and status.
pub struct BackgroundTaskManager {
    tasks: HashMap<TaskId, BackgroundTask>,
    /// Order in which tasks were added (for "most recent" cancellation).
    task_order: Vec<TaskId>,
}

impl BackgroundTaskManager {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            task_order: Vec::new(),
        }
    }

    /// Register a new background task.
    /// Returns the TaskId and a sender for the task to send updates.
    pub fn register_task(&mut self, task_type: TaskType) -> (TaskId, mpsc::Sender<TaskUpdate>, Arc<AtomicBool>) {
        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let task = BackgroundTask::new(task_type, cancel_flag.clone(), rx);
        let id = task.id;

        self.tasks.insert(id, task);
        self.task_order.push(id);

        (id, tx, cancel_flag)
    }

    /// Check if a task of the given type is already running.
    pub fn is_running(&self, task_type: TaskType) -> bool {
        self.tasks.values().any(|t| t.task_type == task_type && t.is_running())
    }

    /// Cancel a specific task by ID.
    pub fn cancel_task(&mut self, id: TaskId) -> bool {
        if let Some(task) = self.tasks.get(&id) {
            if task.is_running() {
                task.cancel();
                return true;
            }
        }
        false
    }

    /// Cancel the most recently started running task.
    /// Returns true if a task was cancelled.
    pub fn cancel_most_recent(&mut self) -> bool {
        // Find the most recent running task
        for id in self.task_order.iter().rev() {
            if let Some(task) = self.tasks.get(id) {
                if task.is_running() {
                    task.cancel();
                    return true;
                }
            }
        }
        false
    }

    /// Cancel all running tasks.
    pub fn cancel_all(&mut self) {
        for task in self.tasks.values() {
            if task.is_running() {
                task.cancel();
            }
        }
    }

    /// Poll all task channels for updates.
    /// Returns completion messages that should be displayed to the user.
    pub fn poll_updates(&mut self) -> Vec<TaskCompletionInfo> {
        let mut completed = Vec::new();

        // Collect updates from all tasks
        let task_ids: Vec<TaskId> = self.tasks.keys().copied().collect();

        for id in task_ids {
            if let Some(task) = self.tasks.get_mut(&id) {
                // Drain all available updates
                while let Ok(update) = task.receiver.try_recv() {
                    match update {
                        TaskUpdate::Started { total } => {
                            task.progress = Some(TaskProgress::new(0, total));
                        }
                        TaskUpdate::Progress(progress) => {
                            task.progress = Some(progress);
                        }
                        TaskUpdate::Completed { message } => {
                            task.state = TaskState::Completed;
                            completed.push(TaskCompletionInfo {
                                id,
                                task_type: task.task_type,
                                message,
                                success: true,
                            });
                        }
                        TaskUpdate::Cancelled => {
                            task.state = TaskState::Cancelled;
                            completed.push(TaskCompletionInfo {
                                id,
                                task_type: task.task_type,
                                message: "Cancelled".to_string(),
                                success: false,
                            });
                        }
                        TaskUpdate::Failed { error } => {
                            task.state = TaskState::Failed(error.clone());
                            completed.push(TaskCompletionInfo {
                                id,
                                task_type: task.task_type,
                                message: error,
                                success: false,
                            });
                        }
                    }
                }
            }
        }

        // Remove completed tasks from tracking
        for info in &completed {
            self.tasks.remove(&info.id);
            self.task_order.retain(|id| *id != info.id);
        }

        completed
    }

    /// Get all running tasks for display.
    pub fn running_tasks(&self) -> Vec<&BackgroundTask> {
        self.task_order
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .filter(|t| t.is_running())
            .collect()
    }

    /// Check if any tasks are running.
    pub fn has_running_tasks(&self) -> bool {
        self.tasks.values().any(|t| t.is_running())
    }

    /// Get task by index in the running tasks list (for TaskList dialog).
    pub fn get_running_task_by_index(&self, index: usize) -> Option<TaskId> {
        self.running_tasks().get(index).map(|t| t.id)
    }
}

impl Default for BackgroundTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

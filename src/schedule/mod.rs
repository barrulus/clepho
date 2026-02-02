//! Schedule manager for executing scheduled tasks.

use chrono::{Local, Timelike, Utc};
use std::time::Instant;

use crate::db::{Database, ScheduleStatus, ScheduledTask};

/// Manages the polling and execution of scheduled tasks.
pub struct ScheduleManager {
    /// Last time we checked for due schedules.
    last_check: Option<Instant>,
    /// Minimum interval between checks (1 second).
    check_interval_ms: u64,
}

impl ScheduleManager {
    pub fn new() -> Self {
        Self {
            last_check: None,
            check_interval_ms: 1000,
        }
    }

    /// Poll for tasks that are ready to execute.
    /// Returns tasks that should be executed now.
    pub fn poll_schedules(&mut self, db: &Database) -> Vec<ScheduledTask> {
        // Rate limit checks to once per second max
        if let Some(last) = self.last_check {
            if last.elapsed().as_millis() < self.check_interval_ms as u128 {
                return Vec::new();
            }
        }
        self.last_check = Some(Instant::now());

        let now = Utc::now();
        let now_str = now.format("%Y-%m-%dT%H:%M:%S").to_string();
        let current_hour = Local::now().hour() as u8;

        // Get pending schedules that are due
        let pending = match db.get_pending_schedules() {
            Ok(tasks) => tasks,
            Err(_) => return Vec::new(),
        };

        // Filter to tasks that are:
        // 1. scheduled_at <= now
        // 2. Within hours of operation (if set)
        pending
            .into_iter()
            .filter(|task| {
                // Check if scheduled time has passed
                if task.scheduled_at > now_str {
                    return false;
                }

                // Check hours of operation if set
                if let (Some(start), Some(end)) = (task.hours_start, task.hours_end) {
                    if start <= end {
                        // Normal range (e.g., 9-17)
                        if current_hour < start || current_hour >= end {
                            return false;
                        }
                    } else {
                        // Overnight range (e.g., 22-6)
                        if current_hour < start && current_hour >= end {
                            return false;
                        }
                    }
                }

                true
            })
            .collect()
    }

    /// Check for overdue schedules (for startup prompt).
    pub fn check_overdue(&self, db: &Database) -> Vec<ScheduledTask> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        db.get_overdue_schedules(&now).unwrap_or_default()
    }
}

impl Default for ScheduleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a scheduled task by delegating to the appropriate task type.
/// Note: The actual task execution is handled by the App, this just
/// marks the task as running and returns the task info.
pub fn mark_task_running(
    task: &ScheduledTask,
    db: &Database,
) -> Result<(), String> {
    // Mark as running
    if let Err(e) = db.update_schedule_status(task.id, ScheduleStatus::Running, None) {
        return Err(format!("Failed to update status: {}", e));
    }
    Ok(())
}

/// Mark a scheduled task as completed.
pub fn mark_task_completed(
    task_id: i64,
    db: &Database,
) -> Result<(), String> {
    if let Err(e) = db.update_schedule_status(task_id, ScheduleStatus::Completed, None) {
        return Err(format!("Failed to update status: {}", e));
    }
    Ok(())
}

/// Mark a scheduled task as failed.
#[allow(dead_code)]
pub fn mark_task_failed(
    task_id: i64,
    db: &Database,
    error: &str,
) -> Result<(), String> {
    if let Err(e) = db.update_schedule_status(task_id, ScheduleStatus::Failed, Some(error)) {
        return Err(format!("Failed to update status: {}", e));
    }
    Ok(())
}

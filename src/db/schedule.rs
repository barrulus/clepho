//! Database operations for scheduled tasks.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::Database;

/// Type of scheduled task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduledTaskType {
    Scan,
    LlmBatch,
    FaceDetection,
}

impl ScheduledTaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScheduledTaskType::Scan => "Scan",
            ScheduledTaskType::LlmBatch => "LlmBatch",
            ScheduledTaskType::FaceDetection => "FaceDetection",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Scan" => Some(ScheduledTaskType::Scan),
            "LlmBatch" => Some(ScheduledTaskType::LlmBatch),
            "FaceDetection" => Some(ScheduledTaskType::FaceDetection),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ScheduledTaskType::Scan => "Directory Scan",
            ScheduledTaskType::LlmBatch => "LLM Batch Process",
            ScheduledTaskType::FaceDetection => "Face Detection",
        }
    }
}

/// Status of a scheduled task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

impl ScheduleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScheduleStatus::Pending => "pending",
            ScheduleStatus::Running => "running",
            ScheduleStatus::Completed => "completed",
            ScheduleStatus::Cancelled => "cancelled",
            ScheduleStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ScheduleStatus::Pending),
            "running" => Some(ScheduleStatus::Running),
            "completed" => Some(ScheduleStatus::Completed),
            "cancelled" => Some(ScheduleStatus::Cancelled),
            "failed" => Some(ScheduleStatus::Failed),
            _ => None,
        }
    }
}

/// A scheduled task record.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScheduledTask {
    pub id: i64,
    pub task_type: ScheduledTaskType,
    pub target_path: String,
    pub photo_ids: Option<Vec<i64>>,
    pub scheduled_at: String,
    pub hours_start: Option<u8>,
    pub hours_end: Option<u8>,
    pub status: ScheduleStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
}

impl Database {
    /// Create a new scheduled task.
    pub fn create_scheduled_task(
        &self,
        task_type: ScheduledTaskType,
        target_path: &str,
        photo_ids: Option<&[i64]>,
        scheduled_at: &str,
        hours_start: Option<u8>,
        hours_end: Option<u8>,
    ) -> Result<i64> {
        let photo_ids_json = photo_ids.map(|ids| {
            serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string())
        });

        self.conn.execute(
            r#"
            INSERT INTO scheduled_tasks (
                task_type, target_path, photo_ids, scheduled_at, hours_start, hours_end
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![
                task_type.as_str(),
                target_path,
                photo_ids_json,
                scheduled_at,
                hours_start,
                hours_end,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all pending scheduled tasks.
    pub fn get_pending_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending'
            ORDER BY scheduled_at ASC
            "#,
        )?;

        let tasks = stmt
            .query_map([], Self::row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Get overdue scheduled tasks (scheduled_at < now and status = pending).
    pub fn get_overdue_schedules(&self, now: &str) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending' AND scheduled_at < ?
            ORDER BY scheduled_at ASC
            "#,
        )?;

        let tasks = stmt
            .query_map([now], Self::row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Get all scheduled tasks (for display).
    #[allow(dead_code)]
    pub fn get_all_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            ORDER BY scheduled_at DESC
            LIMIT 100
            "#,
        )?;

        let tasks = stmt
            .query_map([], Self::row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Update the status of a scheduled task.
    pub fn update_schedule_status(
        &self,
        id: i64,
        status: ScheduleStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

        match status {
            ScheduleStatus::Running => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ?, started_at = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), now, id],
                )?;
            }
            ScheduleStatus::Completed | ScheduleStatus::Failed | ScheduleStatus::Cancelled => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ?, completed_at = ?, error_message = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), now, error_message, id],
                )?;
            }
            ScheduleStatus::Pending => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), id],
                )?;
            }
        }

        Ok(())
    }

    /// Cancel a scheduled task.
    pub fn cancel_schedule(&self, id: i64) -> Result<()> {
        self.update_schedule_status(id, ScheduleStatus::Cancelled, None)
    }

    /// Delete a scheduled task.
    #[allow(dead_code)]
    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM scheduled_tasks WHERE id = ?", [id])?;
        Ok(())
    }

    /// Helper to convert a row to ScheduledTask.
    fn row_to_scheduled_task(row: &rusqlite::Row) -> rusqlite::Result<ScheduledTask> {
        let task_type_str: String = row.get(1)?;
        let task_type = ScheduledTaskType::from_str(&task_type_str)
            .unwrap_or(ScheduledTaskType::Scan);

        let photo_ids_json: Option<String> = row.get(3)?;
        let photo_ids = photo_ids_json.and_then(|json| {
            serde_json::from_str::<Vec<i64>>(&json).ok()
        });

        let status_str: String = row.get(7)?;
        let status = ScheduleStatus::from_str(&status_str)
            .unwrap_or(ScheduleStatus::Pending);

        Ok(ScheduledTask {
            id: row.get(0)?,
            task_type,
            target_path: row.get(2)?,
            photo_ids,
            scheduled_at: row.get(4)?,
            hours_start: row.get(5)?,
            hours_end: row.get(6)?,
            status,
            created_at: row.get(8)?,
            started_at: row.get(9)?,
            completed_at: row.get(10)?,
            error_message: row.get(11)?,
        })
    }
}

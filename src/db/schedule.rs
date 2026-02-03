//! Types for scheduled tasks.

use serde::{Deserialize, Serialize};

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

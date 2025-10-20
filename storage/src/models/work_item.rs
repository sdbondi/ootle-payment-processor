// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: uuid::Uuid,
    pub task: TaskType,
    pub data: serde_json::Value,
    pub status: JobStatus,
    pub attempts: u32,
}

impl Job {
    pub fn is_valid_transition(&self, new_status: JobStatus) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match (&self.status, &new_status) {
            (JobStatus::Pending, JobStatus::InProgress) => true,
            (JobStatus::InProgress, JobStatus::Completed) => true,
            (JobStatus::InProgress, JobStatus::Failed) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JobStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    TimedOut,
    Invalid,
}

impl JobStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::InProgress => "InProgress",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::TimedOut => "TimedOut",
            Self::Invalid => "Invalid",
        }
    }
}

impl FromStr for JobStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Pending" => Ok(Self::Pending),
            "InProgress" => Ok(Self::InProgress),
            "Completed" => Ok(Self::Completed),
            "Failed" => Ok(Self::Failed),
            "TimedOut" => Ok(Self::TimedOut),
            "Invalid" => Ok(Self::Invalid),
            _ => Err(anyhow::anyhow!("Unknown work item status: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TaskType {
    ProcessPayment,
}

impl Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::ProcessPayment => write!(f, "ProcessPayment"),
        }
    }
}

impl FromStr for TaskType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ProcessPayment" => Ok(TaskType::ProcessPayment),
            _ => Err(anyhow::anyhow!("Unknown task type: {}", s)),
        }
    }
}

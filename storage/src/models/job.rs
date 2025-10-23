// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: uuid::Uuid,
    pub job_type: JobType,
    pub execution_time: Duration,
    pub data: serde_json::Value,
    pub status: JobStatus,
    pub attempts: u32,
    pub result: Option<serde_json::Value>,
    pub failure_reason: Option<Box<str>>,
    pub priority: u32,
    pub updated_at: time::OffsetDateTime,
}

impl Job {
    pub fn is_valid_transition(&self, new_status: JobStatus) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match (&self.status, &new_status) {
            (JobStatus::Pending | JobStatus::WaitingForBalance, JobStatus::InProgress) => true,
            (JobStatus::InProgress, JobStatus::Completed) => true,
            (JobStatus::InProgress, JobStatus::Failed) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub enum JobStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    WaitingForBalance,
    TimedOut,
    Invalid,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::InProgress => "InProgress",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::WaitingForBalance => "WaitingForBalance",
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
            "WaitingForBalance" => Ok(Self::WaitingForBalance),
            "TimedOut" => Ok(Self::TimedOut),
            "Invalid" => Ok(Self::Invalid),
            _ => Err(anyhow::anyhow!("Unknown work item status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum JobType {
    ProcessPayment,
}

impl Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::ProcessPayment => write!(f, "ProcessPayment"),
        }
    }
}

impl FromStr for JobType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ProcessPayment" => Ok(JobType::ProcessPayment),
            _ => Err(anyhow::anyhow!("Unknown task type: {}", s)),
        }
    }
}

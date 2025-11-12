// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_payment_processor_storage::models::JobType;
use ootle_payment_processor_storage::{
    ReadableStore, StorageError, StoreReadTransaction, StoreWriteTransaction, WriteableStore,
};
use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub struct Store<TBackend> {
    backend: TBackend,
}

impl<TBackend: ReadableStore + WriteableStore> Store<TBackend> {
    pub fn new(backend: TBackend) -> Self {
        Self { backend }
    }

    pub async fn enqueue_job(
        &self,
        task: JobType,
        data: Option<serde_json::Value>,
        priority: u32,
    ) -> Result<uuid::Uuid, StorageError> {
        let mut tx = self.backend.create_write_tx().await?;
        let task_id = tx.enqueue_work(task, priority, data).await?;
        tx.commit().await?;
        Ok(task_id)
    }

    pub async fn get_job(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ootle_payment_processor_storage::models::Job>, StorageError> {
        let mut tx = self.backend.create_read_tx().await?;
        tx.get_job_by_id(id).await
    }

    pub async fn get_queue_stats(&self) -> Result<QueueStats, StorageError> {
        let mut tx = self.backend.create_read_tx().await?;
        let pending_jobs = tx
            .count_jobs_by_status(ootle_payment_processor_storage::models::JobStatus::Pending)
            .await?;
        let in_progress_jobs = tx
            .count_jobs_by_status(ootle_payment_processor_storage::models::JobStatus::InProgress)
            .await?;
        let completed_jobs = tx
            .count_jobs_by_status(ootle_payment_processor_storage::models::JobStatus::Completed)
            .await?;
        let failed_jobs = tx
            .count_jobs_by_status(ootle_payment_processor_storage::models::JobStatus::Failed)
            .await?;

        Ok(QueueStats {
            pending_jobs,
            in_progress_jobs,
            completed_jobs,
            failed_jobs,
        })
    }
}

pub struct QueueStats {
    pub pending_jobs: u64,
    pub in_progress_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
}

impl Display for QueueStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueueStats {{ pending_jobs: {}, in_progress_jobs: {}, completed_jobs: {}, failed_jobs: {} }}",
            self.pending_jobs, self.in_progress_jobs, self.completed_jobs, self.failed_jobs
        )
    }
}

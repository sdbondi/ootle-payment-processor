// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_payment_processor_storage::models::JobType;
use ootle_payment_processor_storage::{
    ReadableStore, StorageError, StoreReadTransaction, StoreWriteTransaction, WriteableStore,
};

pub struct Store<TBackend> {
    backend: TBackend,
}

impl<TBackend: ReadableStore + WriteableStore> Store<TBackend> {
    pub fn new(backend: TBackend) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &TBackend {
        &self.backend
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
}

// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_payment_processor_storage::models::TaskType;
use ootle_payment_processor_storage::{ReadableStore, StorageError, StoreWriteTransaction, WriteableStore};

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

    pub async fn push_task(&self, task: TaskType, data: Option<serde_json::Value>) -> Result<uuid::Uuid, StorageError> {
        let mut tx = self.backend.create_write_tx().await?;
        let task_id = tx.enqueue_work(task, data).await?;
        tx.commit().await?;
        Ok(task_id)
    }
}

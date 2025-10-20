// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::error::StorageError;
use crate::models::{JobStatus, TaskType};
use crate::reader::ReadableStore;
use std::time::Duration;

pub trait WriteableStore: ReadableStore {
    type WriteTransaction<'a>: StoreWriteTransaction
    where
        Self: 'a;

    fn create_write_tx(&self) -> impl Future<Output = Result<Self::WriteTransaction<'_>, StorageError>> + Send;

    fn with_write_tx<F, R, E, TFut>(&self, f: F) -> impl Future<Output = Result<R, E>>
    where
        E: From<StorageError>,
        F: FnOnce(&mut Self::WriteTransaction<'_>) -> TFut,
        TFut: Future<Output = Result<R, E>>,
    {
        async move {
            let mut tx = self.create_write_tx().await?;
            match f(&mut tx).await {
                Ok(r) => {
                    tx.commit().await?;
                    Ok(r)
                },
                Err(e) => {
                    if let Err(err) = tx.rollback().await {
                        log::error!("Failed to rollback transaction: {}", err);
                    }
                    Err(e)
                },
            }
        }
    }
}

pub trait StoreWriteTransaction {
    fn enqueue_work(
        &mut self,
        task: TaskType,
        data: Option<serde_json::Value>,
    ) -> impl Future<Output = Result<uuid::Uuid, StorageError>> + Send;

    fn set_job_status(
        &mut self,
        id: &uuid::Uuid,
        execution_time: Duration,
        status: JobStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;
    fn delete_job(&mut self, id: &uuid::Uuid) -> impl Future<Output = Result<(), StorageError>> + Send;
    fn commit(self) -> impl Future<Output = Result<(), StorageError>> + Send;
    fn rollback(self) -> impl Future<Output = Result<(), StorageError>> + Send;
}

// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::PostgresStore;
use crate::PostgresReadTransaction;
use ootle_payment_processor_storage::models::{JobStatus, TaskType};
use ootle_payment_processor_storage::{AsReadable, StorageError, WriteableStore};
use sqlx::types::{uuid, Uuid};
use sqlx::PgConnection;
use std::time::Duration;

impl WriteableStore for PostgresStore {
    type WriteTransaction<'a> = PostgresWriteTransaction<'a>;

    async fn create_write_tx(&self) -> Result<Self::WriteTransaction<'_>, StorageError> {
        self.pool
            .begin()
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })
            .map(|tx| PostgresReadTransaction { transaction: tx })
            .map(|read_tx| PostgresWriteTransaction { transaction: read_tx })
    }
}

pub struct PostgresWriteTransaction<'tx> {
    transaction: PostgresReadTransaction<'tx>,
}

impl PostgresWriteTransaction<'_> {
    fn conn(&mut self) -> &mut PgConnection {
        self.transaction.conn()
    }
}

impl ootle_payment_processor_storage::StoreWriteTransaction for PostgresWriteTransaction<'_> {
    async fn enqueue_work(
        &mut self,
        task: TaskType,
        data: Option<serde_json::Value>,
    ) -> Result<uuid::Uuid, StorageError> {
        let a = sqlx::query!(
            "INSERT INTO job_queue (task, status, payload, attempts) VALUES ($1, $2, $3, $4) RETURNING id",
            task.to_string(),
            "Pending",
            data,
            0i32
        )
        .fetch_one(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(a.id)
    }

    async fn set_job_status(
        &mut self,
        id: &Uuid,
        execution_time: Duration,
        status: JobStatus,
    ) -> Result<(), StorageError> {
        sqlx::query!(
            "UPDATE job_queue SET status = $1, execute_time_ms = $2, updated_at = now() WHERE id = $3",
            status.as_str(),
            i64::try_from(execution_time.as_millis()).unwrap_or(i64::MAX),
            id
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(())
    }

    async fn delete_job(&mut self, id: &Uuid) -> Result<(), StorageError> {
        sqlx::query!("DELETE FROM job_queue WHERE id = $1", id)
            .execute(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(())
    }

    async fn commit(self) -> Result<(), StorageError> {
        self.transaction
            .transaction
            .commit()
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })
    }

    async fn rollback(self) -> Result<(), StorageError> {
        self.transaction
            .transaction
            .rollback()
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })
    }
}

impl<'tx> AsReadable for PostgresWriteTransaction<'tx> {
    type Store = PostgresReadTransaction<'tx>;

    fn as_readable(&mut self) -> &mut Self::Store {
        &mut self.transaction
    }
}

// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::SqliteStore;
use crate::SqliteReadTransaction;
use ootle_payment_processor_storage::models::{JobStatus, JobType};
use ootle_payment_processor_storage::{AsReadable, StorageError, WriteableStore};
use serde_json::Value;
use sqlx::types::{uuid, Uuid};
use sqlx::SqliteConnection;
use std::time::Duration;
use tari_template_lib::models::ResourceAddress;

impl WriteableStore for SqliteStore {
    type WriteTransaction<'a> = SqliteWriteTransaction<'a>;

    async fn create_write_tx(&self) -> Result<Self::WriteTransaction<'_>, StorageError> {
        self.pool
            .begin()
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })
            .map(|tx| SqliteReadTransaction { transaction: tx })
            .map(|read_tx| SqliteWriteTransaction { transaction: read_tx })
    }
}

pub struct SqliteWriteTransaction<'tx> {
    transaction: SqliteReadTransaction<'tx>,
}

impl SqliteWriteTransaction<'_> {
    fn conn(&mut self) -> &mut SqliteConnection {
        self.transaction.conn()
    }
}

impl ootle_payment_processor_storage::StoreWriteTransaction for SqliteWriteTransaction<'_> {
    async fn enqueue_work(
        &mut self,
        task: JobType,
        priority: u32,
        data: Option<serde_json::Value>,
    ) -> Result<uuid::Uuid, StorageError> {
        let uuid = uuid::Uuid::new_v4();
        let uuid_str = uuid.to_string();
        let task = task.to_string();
        let priority = priority as i32;
        sqlx::query!(
            "INSERT INTO job_queue (uuid, task, status, payload, attempts, priority) VALUES ($1, $2, $3, $4, $5, $6)",
            uuid_str,
            task,
            "Pending",
            data,
            0i32,
            priority,
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(uuid)
    }

    async fn job_set_status(&mut self, id: &Uuid, status: JobStatus) -> Result<(), StorageError> {
        let id_str = id.to_string();
        let status_str = status.as_str();
        sqlx::query!(
            "UPDATE job_queue SET status = $1, updated_at = CURRENT_TIMESTAMP WHERE uuid = $2",
            status_str,
            id_str
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(())
    }

    async fn job_requeue_for_balance(
        &mut self,
        id: &Uuid,
        resource_address: ResourceAddress,
        amount: u64,
    ) -> Result<(), StorageError> {
        let resx = resource_address.to_string();
        let amount = amount as i64;

        sqlx::query!(
            "UPDATE job_queue \
                SET \
                    status = 'WaitingForBalance', \
                    await_balance_resx = $1, \
                    await_balance_amt = $2, \
                    attempts = attempts + 1, \
                    updated_at = CURRENT_TIMESTAMP \
                WHERE id = $3",
            resx,
            amount,
            id
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(())
    }

    async fn set_failure_reason(&mut self, id: &Uuid, reason: String) -> Result<(), StorageError> {
        let id_str = id.to_string();
        sqlx::query!(
            "UPDATE job_queue SET status = 'Failed', failure_reason = $1, updated_at = CURRENT_TIMESTAMP WHERE uuid = $2",
            reason,
            id_str
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(())
    }

    async fn job_set_completed_result(
        &mut self,
        id: &Uuid,
        execution_time: Duration,
        result: Value,
    ) -> Result<(), StorageError> {
        let exec_ms = i64::try_from(execution_time.as_millis()).unwrap_or(i64::MAX);
        let id_str = id.to_string();
        sqlx::query!(
            "UPDATE job_queue SET status = 'Completed', result = $1, execute_time_ms = $2, updated_at = CURRENT_TIMESTAMP WHERE uuid = $3",
            result,
            exec_ms,
            id_str
        )
            .execute(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(())
    }

    async fn job_requeue_for_later(&mut self, id: &Uuid, scheduled_at: Duration) -> Result<u32, StorageError> {
        #[allow(clippy::cast_possible_truncation)]
        let secs = if scheduled_at.as_secs() > f64::MAX.floor() as u64 {
            f64::MAX.floor()
        } else {
            scheduled_at.as_secs() as f64
        };
        let plus_secs = format!("+{} seconds", secs);
        let id_str = id.to_string();
        let rec = sqlx::query!(
            "UPDATE job_queue \
                SET \
                    attempts = attempts + 1, \
                    status = 'Pending', \
                    scheduled_at = DATETIME(CURRENT_TIMESTAMP, $2), \
                    updated_at = CURRENT_TIMESTAMP \
                WHERE uuid = $1 RETURNING attempts",
            id_str,
            plus_secs,
        )
        .fetch_one(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(u32::try_from(rec.attempts as u64).unwrap_or(u32::MAX))
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

impl<'tx> AsReadable for SqliteWriteTransaction<'tx> {
    type Store = SqliteReadTransaction<'tx>;

    fn as_readable(&mut self) -> &mut Self::Store {
        &mut self.transaction
    }
}

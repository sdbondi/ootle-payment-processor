// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::SqliteStore;
use crate::SqliteStorageError;
use ootle_payment_processor_storage::models::{Job, JobStatus};
use ootle_payment_processor_storage::{AsReadable, ReadableStore, StorageError};
use sqlx::types::{uuid, Uuid};
use sqlx::{Error, SqliteConnection};
use std::str::FromStr;
use std::time::Duration;
use tari_template_lib::models::ResourceAddress;

impl ReadableStore for SqliteStore {
    type ReadTransaction<'tx> = SqliteReadTransaction<'tx>;

    async fn create_read_tx(&self) -> Result<Self::ReadTransaction<'_>, StorageError> {
        self.pool
            .begin()
            .await
            .map_err(|e| match e {
                Error::RowNotFound => StorageError::NotFound,
                e => StorageError::DatabaseError { source: e.into() },
            })
            .map(|tx| SqliteReadTransaction { transaction: tx })
    }
}

pub struct SqliteReadTransaction<'tx> {
    pub(super) transaction: sqlx::Transaction<'tx, sqlx::Sqlite>,
}

impl<'tx> SqliteReadTransaction<'tx> {
    pub(super) fn conn(&mut self) -> &mut SqliteConnection {
        &mut self.transaction
    }
}

impl ootle_payment_processor_storage::StoreReadTransaction for SqliteReadTransaction<'_> {
    async fn get_next_job_id(&mut self) -> Result<Option<uuid::Uuid>, StorageError> {
        let result = sqlx::query!(
            "SELECT uuid FROM job_queue WHERE status = 'Pending' AND scheduled_at <= CURRENT_TIMESTAMP ORDER BY priority DESC, created_at ASC",
        )
        .fetch_optional(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        let Some(result) = result else {
            return Ok(None);
        };

        let uuid =
            uuid::Uuid::from_str(&result.uuid).map_err(|e| SqliteStorageError::DecodeError { source: e.into() })?;

        Ok(Some(uuid))
    }

    async fn get_job_by_id(&mut self, id: Uuid) -> Result<Option<Job>, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query!("SELECT * FROM job_queue WHERE uuid = $1", id_str)
            .fetch_optional(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        let Some(result) = result else {
            return Ok(None);
        };

        Ok(Some(Job {
            id: result.uuid.parse().map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse UUID: {}", e),
            })?,
            job_type: result.task.parse().map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse task: {}", e),
            })?,
            status: result.status.parse().map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse status: {}", e),
            })?,
            execution_time: Duration::from_millis(result.execute_time_ms as u64),
            updated_at: result.updated_at,
            attempts: u32::try_from(result.attempts as u64).map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to convert attempts to u32: {}", e),
            })?,
            data: serde_json::from_str(&result.payload).map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse payload JSON: {}", e),
            })?,
            failure_reason: result.failure_reason.map(|s| s.into_boxed_str()),
            priority: u32::try_from(result.priority as u64).map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to convert priority to u32: {}", e),
            })?,
            result: result
                .result
                .as_ref()
                .map(|s| serde_json::from_str(s))
                .transpose()
                .map_err(|e| SqliteStorageError::DecodeError {
                    source: anyhow::anyhow!("Failed to parse result JSON: {}", e),
                })?,
        }))
    }

    async fn get_next_job_waiting_for_balance(
        &mut self,
        resource_address: ResourceAddress,
        amount: u64,
    ) -> Result<Option<Uuid>, StorageError> {
        let resx = resource_address.to_string();
        let amount = amount as i64;
        let result = sqlx::query!(
            "SELECT uuid FROM job_queue WHERE status = 'WaitingForBalance' AND await_balance_resx = $1 AND await_balance_amt <= $2 ORDER BY priority DESC, created_at ASC",
            resx,
            amount,
        )
            .fetch_optional(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        let uuid = result
            .map(|r| r.uuid.parse())
            .transpose()
            .map_err(|e| SqliteStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse UUID: {}", e),
            })?;
        Ok(uuid)
    }

    async fn count_jobs_by_status(&mut self, status: JobStatus) -> Result<u64, StorageError> {
        let status_str = status.as_str();
        let result = sqlx::query!("SELECT COUNT(*) as count FROM job_queue WHERE status = $1", status_str,)
            .fetch_one(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(result.count as u64)
    }
}

impl AsReadable for SqliteReadTransaction<'_> {
    type Store = Self;

    fn as_readable(&mut self) -> &mut Self::Store {
        self
    }
}

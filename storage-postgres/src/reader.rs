// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::PostgresStore;
use crate::PostgresStorageError;
use ootle_payment_processor_storage::models::{Job, JobStatus};
use ootle_payment_processor_storage::{AsReadable, ReadableStore, StorageError};
use sqlx::types::{uuid, Uuid};
use sqlx::{Error, PgConnection};
use std::time::Duration;
use tari_template_lib::models::ResourceAddress;

impl ReadableStore for PostgresStore {
    type ReadTransaction<'tx> = PostgresReadTransaction<'tx>;

    async fn create_read_tx(&self) -> Result<Self::ReadTransaction<'_>, StorageError> {
        self.pool
            .begin()
            .await
            .map_err(|e| match e {
                Error::RowNotFound => StorageError::NotFound,
                e => StorageError::DatabaseError { source: e.into() },
            })
            .map(|tx| PostgresReadTransaction { transaction: tx })
    }
}

pub struct PostgresReadTransaction<'tx> {
    pub(super) transaction: sqlx::Transaction<'tx, sqlx::Postgres>,
}

impl<'tx> PostgresReadTransaction<'tx> {
    pub(super) fn conn(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }
}

impl ootle_payment_processor_storage::StoreReadTransaction for PostgresReadTransaction<'_> {
    async fn get_next_job_id(&mut self) -> Result<Option<uuid::Uuid>, StorageError> {
        let result = sqlx::query!(
            "SELECT id FROM job_queue WHERE status = 'Pending' AND scheduled_at <= now() ORDER BY priority DESC, created_at ASC",
        )
        .fetch_optional(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        let Some(result) = result else {
            return Ok(None);
        };

        Ok(Some(result.id))
    }

    async fn get_job_by_id(&mut self, id: Uuid) -> Result<Option<Job>, StorageError> {
        let result = sqlx::query!("SELECT * FROM job_queue WHERE id = $1", id)
            .fetch_optional(self.conn())
            .await
            .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        let Some(result) = result else {
            return Ok(None);
        };

        Ok(Some(Job {
            id: result.id,
            job_type: result.task.parse().map_err(|e| PostgresStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse task: {}", e),
            })?,
            status: result.status.parse().map_err(|e| PostgresStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse status: {}", e),
            })?,
            execution_time: Duration::from_millis(result.execute_time_ms as u64),
            updated_at: result.updated_at,
            attempts: result.attempts as u32,
            data: result.payload,
            failure_reason: result.failure_reason.map(|s| s.into_boxed_str()),
            priority: result.priority as u32,
            result: result.result,
        }))
    }

    async fn get_next_job_waiting_for_balance(
        &mut self,
        resource_address: ResourceAddress,
        amount: u64,
    ) -> Result<Option<Uuid>, StorageError> {
        let result = sqlx::query!(
            "SELECT id FROM job_queue WHERE status = 'WaitingForBalance' AND await_balance_resx = $1 AND await_balance_amt <= $2 ORDER BY priority DESC, created_at ASC",
            resource_address.to_string(),
            amount as i64,
        )
        .fetch_optional(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(result.map(|r| r.id))
    }

    async fn count_jobs_by_status(&mut self, status: JobStatus) -> Result<u64, StorageError> {
        let result = sqlx::query!(
            "SELECT COUNT(*) as count FROM job_queue WHERE status = $1",
            status.as_str()
        )
        .fetch_one(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(result.count.unwrap_or_default() as u64)
    }
}

impl AsReadable for PostgresReadTransaction<'_> {
    type Store = Self;

    fn as_readable(&mut self) -> &mut Self::Store {
        self
    }
}

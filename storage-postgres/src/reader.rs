// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::PostgresStore;
use crate::PostgresStorageError;
use ootle_payment_processor_storage::models::Job;
use ootle_payment_processor_storage::{AsReadable, ReadableStore, StorageError};
use sqlx::types::{uuid, Uuid};
use sqlx::{Error, PgConnection};

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
        let result = sqlx::query!("SELECT id FROM job_queue WHERE status = 'Pending' ORDER BY id ASC",)
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
            task: result.task.parse().map_err(|e| PostgresStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse task: {}", e),
            })?,
            status: result.status.parse().map_err(|e| PostgresStorageError::DecodeError {
                source: anyhow::anyhow!("Failed to parse status: {}", e),
            })?,
            attempts: result.attempts as u32,
            data: result.payload,
        }))
    }
}

impl AsReadable for PostgresReadTransaction<'_> {
    type Store = Self;

    fn as_readable(&mut self) -> &mut Self::Store {
        self
    }
}

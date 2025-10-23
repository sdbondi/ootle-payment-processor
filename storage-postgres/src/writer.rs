// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::store::PostgresStore;
use crate::PostgresReadTransaction;
use ootle_payment_processor_storage::models::{JobStatus, JobType};
use ootle_payment_processor_storage::{AsReadable, StorageError, WriteableStore};
use serde_json::Value;
use sqlx::types::{uuid, Uuid};
use sqlx::PgConnection;
use std::time::Duration;
use tari_template_lib::models::ResourceAddress;

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
        task: JobType,
        priority: u32,
        data: Option<serde_json::Value>,
    ) -> Result<uuid::Uuid, StorageError> {
        let a = sqlx::query!(
            "INSERT INTO job_queue (task, status, payload, attempts, priority) VALUES ($1, $2, $3, $4, $5) RETURNING id",
            task.to_string(),
            "Pending",
            data,
            0i32,
            priority as i32
        )
        .fetch_one(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;
        Ok(a.id)
    }

    async fn job_set_status(&mut self, id: &Uuid, status: JobStatus) -> Result<(), StorageError> {
        sqlx::query!(
            "UPDATE job_queue SET status = $1, updated_at = now() WHERE id = $2",
            status.as_str(),
            id
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
        sqlx::query!(
            "UPDATE job_queue \
                SET \
                    status = 'WaitingForBalance', \
                    await_balance_resx = $1, \
                    await_balance_amt = $2, \
                    attempts = attempts + 1, \
                    updated_at = now() \
                WHERE id = $3",
            resource_address.to_string(),
            amount as i64,
            id
        )
        .execute(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(())
    }

    async fn set_failure_reason(&mut self, id: &Uuid, reason: String) -> Result<(), StorageError> {
        sqlx::query!(
            "UPDATE job_queue SET status = 'Failed', failure_reason = $1, updated_at = now() WHERE id = $2",
            reason,
            id
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
        sqlx::query!(
            "UPDATE job_queue SET status = 'Completed', result = $1, execute_time_ms = $2, updated_at = now() WHERE id = $3",
            result,
            i64::try_from(execution_time.as_millis()).unwrap_or(i64::MAX),
            id
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
        let rec = sqlx::query!(
            "UPDATE job_queue \
                SET \
                    attempts = attempts + 1, \
                    status = 'Pending', \
                    scheduled_at = now() + make_interval(secs => $2), \
                    updated_at = now() \
                WHERE id = $1 RETURNING attempts",
            id,
            secs
        )
        .fetch_one(self.conn())
        .await
        .map_err(|e| StorageError::DatabaseError { source: e.into() })?;

        Ok(rec.attempts as u32)
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

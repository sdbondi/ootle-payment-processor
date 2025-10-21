// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_payment_processor_storage::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum PostgresStorageError {
    #[error("Item not found")]
    NotFound,
    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),
    #[error("SQLx migration error: {0}")]
    SqlxMigrateError(#[from] sqlx::migrate::MigrateError),
    #[error("Decode error: {source}")]
    DecodeError { source: anyhow::Error },
}

impl From<PostgresStorageError> for StorageError {
    fn from(err: PostgresStorageError) -> Self {
        match &err {
            PostgresStorageError::NotFound | PostgresStorageError::SqlxError(sqlx::Error::RowNotFound) => {
                StorageError::NotFound
            },
            _ => StorageError::DatabaseError { source: err.into() },
        }
    }
}

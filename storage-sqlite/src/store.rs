// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::error::SqliteStorageError;

#[derive(Debug, Clone)]
pub struct SqliteStore {
    pub(crate) pool: sqlx::SqlitePool,
}

impl SqliteStore {
    pub async fn connect(connection_string: &str) -> Result<Self, SqliteStorageError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect(connection_string)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), SqliteStorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn test_connection(&self) -> Result<(), SqliteStorageError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

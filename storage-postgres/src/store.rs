// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::error::PostgresStorageError;

#[derive(Debug, Clone)]
pub struct PostgresStore {
    pub(crate) pool: sqlx::PgPool,
}

impl PostgresStore {
    pub async fn connect(connection_string: &str) -> Result<Self, PostgresStorageError> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(connection_string)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), PostgresStorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn test_connection(&self) -> Result<(), PostgresStorageError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

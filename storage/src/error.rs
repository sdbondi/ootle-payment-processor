// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Item not found")]
    NotFound,
    #[error("Database error: {source}")]
    DatabaseError { source: anyhow::Error },
}

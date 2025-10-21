// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_ootle_common_types::optional::IsNotFoundError;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Item not found")]
    NotFound,
    #[error("Database error: {source}")]
    DatabaseError { source: anyhow::Error },
}

impl IsNotFoundError for StorageError {
    fn is_not_found_error(&self) -> bool {
        matches!(self, StorageError::NotFound)
    }
}

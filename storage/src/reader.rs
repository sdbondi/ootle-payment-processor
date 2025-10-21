// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::error::StorageError;
use crate::models::Job;

pub trait ReadableStore {
    type ReadTransaction<'tx>: StoreReadTransaction
    where
        Self: 'tx;

    fn create_read_tx(&self) -> impl Future<Output = Result<Self::ReadTransaction<'_>, StorageError>> + Send;

    // fn with_read_tx<F, R, E, TFut>(&self, f: F) -> impl Future<Output = Result<R, E>> + Send
    // where
    //     E: From<StorageError>,
    //     for<'a> F: FnOnce(&'a mut Self::ReadTransaction<'_>) -> TFut,
    //     TFut: Future<Output = Result<R, E>> + Send,
    //     Self: Sync,
    // {
    //     async {
    //         let mut tx = self.create_read_tx().await?;
    //         let ret = f(&mut tx).await?;
    //         Ok(ret)
    //     }
    // }
}

pub trait AsReadable {
    type Store: StoreReadTransaction;

    fn as_readable(&mut self) -> &mut Self::Store;
}

pub trait StoreReadTransaction {
    fn get_next_job_id(&mut self) -> impl Future<Output = Result<Option<uuid::Uuid>, StorageError>> + Send;
    fn get_job_by_id(&mut self, id: uuid::Uuid) -> impl Future<Output = Result<Option<Job>, StorageError>> + Send;
}

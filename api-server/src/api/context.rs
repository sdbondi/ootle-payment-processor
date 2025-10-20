//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use crate::event::PaymentProcessorEvent;
use crate::startup::App;
use crate::store::Store;
use ootle_payment_processor_storage_postgres::PostgresStore;
use std::sync::Arc;

#[derive(Clone)]
pub struct HandlerContext {
    inner: Arc<App>,
}

impl HandlerContext {
    pub fn new(app: App) -> Self {
        Self { inner: Arc::new(app) }
    }

    pub fn store(&self) -> &Store<PostgresStore> {
        &self.inner.store
    }

    pub fn notify<E: Into<PaymentProcessorEvent>>(&self, event: E) {
        self.inner.notify.notify(event)
    }

    pub fn is_worker_running(&self) -> bool {
        !self.inner.task_worker_handle.is_finished()
    }
}

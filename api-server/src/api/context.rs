//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use crate::event::PaymentProcessorEvent;
use crate::startup::App;
use crate::wallet::Wallet;
use std::sync::Arc;

#[cfg(feature = "postgres-storage")]
type Store = crate::store::Store<ootle_payment_processor_storage_postgres::PostgresStore>;
#[cfg(feature = "sqlite-storage")]
type Store = crate::store::Store<ootle_payment_processor_storage_sqlite::SqliteStore>;

#[derive(Clone)]
pub struct HandlerContext {
    inner: Arc<App>,
}

impl HandlerContext {
    pub fn new(app: App) -> Self {
        Self { inner: Arc::new(app) }
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn notify<E: Into<PaymentProcessorEvent>>(&self, event: E) {
        self.inner.notify.notify(event)
    }

    pub fn wallet(&self) -> &Wallet {
        &self.inner.wallet
    }

    pub fn is_worker_running(&self) -> bool {
        !self.inner.task_worker_join_handle.is_finished()
    }

    pub fn is_account_monitor_running(&self) -> bool {
        !self.inner.account_monitor_join_handle.is_finished()
    }

    pub fn is_utxo_scanner_running(&self) -> bool {
        !self.inner.utxo_recovery_join_handle.is_finished()
    }

    pub fn is_stealth_scanner_running(&self) -> bool {
        !self.inner.stealth_scanner_join_handle.is_finished()
    }
}

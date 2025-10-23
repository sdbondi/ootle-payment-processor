//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use crate::event::PaymentProcessorEvent;
use crate::startup::App;
use crate::wallet::Wallet;
use ootle_payment_processor_storage_postgres::PostgresStore;
use std::sync::Arc;
use tari_ootle_wallet_sdk_services::account_monitor::AccountMonitorHandle;
use tari_ootle_wallet_sdk_services::notify::Notify;
use tokio::task;

#[cfg(feature = "postgres-storage")]
type Store = crate::store::Store<ootle_payment_processor_storage_postgres::PostgresStore>;
#[cfg(feature = "sqlite-storage")]
type Store = crate::store::Store<ootle_payment_processor_storage_sqlite::SqliteStore>;

#[derive(Clone)]
pub struct HandlerContext {
    inner: Arc<ContextInner>,
}

impl HandlerContext {
    pub fn new(app: &App) -> Self {
        Self {
            inner: Arc::new(app.into()),
        }
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
        self.inner
            .task_worker_join_handle
            .as_ref()
            .is_some_and(|t| !t.is_finished())
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

struct ContextInner {
    _account_monitor_handle: AccountMonitorHandle,
    wallet: Wallet,
    #[cfg(feature = "postgres-storage")]
    store: crate::store::Store<PostgresStore>,
    #[cfg(feature = "sqlite-storage")]
    store: crate::store::Store<SqliteStore>,
    notify: Notify<PaymentProcessorEvent>,
    task_worker_join_handle: Option<task::AbortHandle>,
    stealth_scanner_join_handle: task::AbortHandle,
    utxo_recovery_join_handle: task::AbortHandle,
    account_monitor_join_handle: task::AbortHandle,
}

impl From<&App> for ContextInner {
    fn from(app: &App) -> Self {
        Self {
            _account_monitor_handle: app._account_monitor_handle.clone(),
            wallet: app.wallet.clone(),
            store: app.store.clone(),
            notify: app.notify.clone(),
            task_worker_join_handle: app.task_worker_join_handle.as_ref().map(|j| j.abort_handle()),
            stealth_scanner_join_handle: app.stealth_scanner_join_handle.abort_handle(),
            utxo_recovery_join_handle: app.utxo_recovery_join_handle.abort_handle(),
            account_monitor_join_handle: app.account_monitor_join_handle.abort_handle(),
        }
    }
}

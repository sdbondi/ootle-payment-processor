// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::cli::Cli;
use crate::event::PaymentProcessorEvent;
use crate::store::Store;
use crate::wallet::Wallet;
use crate::worker::TaskWorker;
use log::*;
use ootle_payment_processor_storage_postgres::PostgresStore;
use tari_ootle_common_types::optional::Optional;
use tari_ootle_wallet_sdk::cipher_seed::CipherSeedRestore;
use tari_ootle_wallet_sdk::{WalletSdk, WalletSdkConfig};
use tari_ootle_wallet_sdk_services::account_monitor::{AccountMonitor, AccountMonitorHandle};
use tari_ootle_wallet_sdk_services::indexer_rest_api::IndexerRestApiNetworkInterface;
use tari_ootle_wallet_sdk_services::notify::Notify;
use tari_ootle_wallet_sdk_services::utxo_scanner::StealthUtxoScannerWorker;
use tari_ootle_wallet_sdk_services::ShutdownSignal;
use tari_ootle_wallet_storage_sqlite::SqliteWalletStore;

pub async fn init_app(cli: &Cli, shutdown: ShutdownSignal) -> anyhow::Result<App> {
    let store = PostgresStore::connect(&cli.connection_string).await?;
    store.migrate().await?;

    let sdk_store = SqliteWalletStore::try_open(cli.sdk_store_path.as_ref())?;
    sdk_store.run_migrations()?;
    let indexer_interface = IndexerRestApiNetworkInterface::new(cli.indexer_api_url.clone());
    let config = WalletSdkConfig {
        network: cli.network,
        // TODO: Figure out how better to secure this. Maybe os keyring is fine (then set this to None). For now, keeping this simple.
        override_keyring_password: Some("ootle-wallet-sdk".into()),
    };

    let mut sdk = WalletSdk::initialize(sdk_store, indexer_interface, config)?;
    sdk.initialize_cipher_seed(CipherSeedRestore::CreateNewIfRequired)?;

    let mut wallet = Wallet::new(sdk.clone());
    let maybe_account = wallet.get_account_or_default(Some("payment-processor")).optional()?;
    let account = maybe_account
        .map(Ok)
        .unwrap_or_else(|| wallet.create_account("payment-processor", true))?;
    info!("💰️ 💰️ 💰️ 💰️ FUNDING ADDRESS: {} 💰️ 💰️ 💰️ 💰️", account.address());

    let notify = Notify::new(1000);

    let scanner = StealthUtxoScannerWorker::new(sdk.clone(), notify.clone());
    let (join, handle) = scanner.spawn();
    let (monitor, account_monitor_handle) = AccountMonitor::new(notify, sdk, handle, shutdown);
    tokio::spawn(monitor.run());

    let notify = Notify::new(1000);
    let handle = TaskWorker::new(store.clone(), wallet, notify.subscribe()).spawn();

    Ok(App {
        account_monitor_handle,
        store: Store::new(store),
        notify,
        task_worker_handle: handle,
    })
}

pub(crate) struct App {
    pub account_monitor_handle: AccountMonitorHandle,
    pub store: Store<PostgresStore>,
    pub notify: Notify<PaymentProcessorEvent>,
    pub task_worker_handle: tokio::task::JoinHandle<()>,
}

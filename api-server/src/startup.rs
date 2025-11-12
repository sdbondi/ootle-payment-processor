// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::cli::Cli;
use crate::event::PaymentProcessorEvent;
use crate::store::Store;
use crate::wallet::Wallet;
use crate::worker::TaskWorker;
use anyhow::Context;
use log::*;
#[cfg(feature = "postgres-storage")]
use ootle_payment_processor_storage_postgres::PostgresStore;
#[cfg(feature = "sqlite-storage")]
use ootle_payment_processor_storage_sqlite::SqliteStore;
use std::time::Duration;
use tari_ootle_common_types::optional::Optional;
use tari_ootle_wallet_sdk::cipher_seed::CipherSeedRestore;
use tari_ootle_wallet_sdk::constants::XTR;
use tari_ootle_wallet_sdk::models::EpochBirthday;
use tari_ootle_wallet_sdk::{WalletSdk, WalletSdkConfig};
use tari_ootle_wallet_sdk_services::account_monitor::{AccountMonitor, AccountMonitorHandle, AccountScanner};
use tari_ootle_wallet_sdk_services::indexer_rest_api::IndexerRestApiNetworkInterface;
use tari_ootle_wallet_sdk_services::notify::Notify;
use tari_ootle_wallet_sdk_services::utxo_scanner::{StealthUtxoScannerWorker, UtxoRecovery};
use tari_ootle_wallet_sdk_services::ShutdownSignal;
use tari_ootle_wallet_storage_sqlite::SqliteWalletStore;
use tokio::task;

#[cfg(not(any(feature = "sqlite-storage", feature = "postgres-storage")))]
compile_error!("At least one storage backend feature must be enabled: sqlite-storage, postgres-storage");

pub async fn init_app(cli: &Cli, shutdown: ShutdownSignal) -> anyhow::Result<App> {
    let sdk_store = SqliteWalletStore::try_open(cli.sdk_store_path.as_ref())?;
    sdk_store.run_migrations()?;
    let indexer_interface = IndexerRestApiNetworkInterface::new(cli.indexer_api_url.clone());
    let config = WalletSdkConfig {
        network: cli.network,
        // TODO: Figure out how better to secure this. Maybe os keyring is fine (then set this to None). For now, keeping this simple.
        override_keyring_password: Some("ootle-wallet-sdk".into()),
    };

    let mut sdk = WalletSdk::initialize(sdk_store, indexer_interface, config, EpochBirthday::far_future())?;
    sdk.initialize_cipher_seed(CipherSeedRestore::CreateNewIfRequired)?;

    if !sdk.resources_api().exists(&XTR)? {
        let resource = sdk
            .substate_api()
            .fetch_resource(XTR)
            .await
            .context("Failed to fetch XTR resource. This may indicate a problem with the indexer connection.")?;
        sdk.resources_api().upsert_resource(&XTR, &resource)?;
    }

    let maybe_account = sdk.accounts_api().get_account_by_name("payment-processor").optional()?;
    let account = maybe_account.map(Ok).unwrap_or_else(|| {
        let address = sdk.key_manager_api().next_account_address()?;
        sdk.accounts_api()
            .create_account(Some("payment-processor"), true, address)
    })?;
    info!("💰️ 💰️ 💰️ 💰️ FUNDING ADDRESS: {} 💰️ 💰️ 💰️ 💰️", account.address());

    let wallet_notify = Notify::new(1000);

    let scanner = StealthUtxoScannerWorker::new(sdk.clone(), wallet_notify.clone());
    let (stealth_scanner_join_handle, utxo_scanner_handle) = scanner.spawn();

    let utxo_recovery_join_handle = {
        let notify_sub = utxo_scanner_handle.subscribe_notifications();
        tokio::spawn(UtxoRecovery::new(sdk.clone()).run(notify_sub))
    };

    let (monitor, account_monitor_handle) = AccountMonitor::new(
        wallet_notify.clone(),
        sdk.clone(),
        utxo_scanner_handle,
        shutdown.clone(),
    );
    let account_monitor_join_handle = tokio::spawn(monitor.with_periodic_scan_interval(Duration::from_secs(20)).run());
    let account_scanner = AccountScanner::new(wallet_notify, sdk.clone());
    // let (tx_service, transaction_service_handle) = TransactionService::new(wallet_notify, sdk.clone(), shutdown);
    // let tx_service_join_handle = task::spawn(tx_service.run());

    let notify = Notify::new(1000);

    #[cfg(feature = "postgres-storage")]
    let store = PostgresStore::connect(&cli.connection_string).await?;

    #[cfg(feature = "sqlite-storage")]
    let store = SqliteStore::connect(&cli.connection_string).await?;

    store.migrate().await?;

    let wallet = Wallet::new(sdk, account_scanner);

    let handle = (!cli.idle).then(|| TaskWorker::new(store.clone(), wallet.clone(), notify.subscribe()).spawn());

    Ok(App {
        _account_monitor_handle: account_monitor_handle,
        wallet,
        store: Store::new(store),
        notify,
        task_worker_join_handle: handle,
        stealth_scanner_join_handle,
        utxo_recovery_join_handle,
        account_monitor_join_handle,
    })
}

pub(crate) struct App {
    pub _account_monitor_handle: AccountMonitorHandle,
    pub wallet: Wallet,
    #[cfg(feature = "postgres-storage")]
    pub store: Store<PostgresStore>,
    #[cfg(feature = "sqlite-storage")]
    pub store: Store<SqliteStore>,
    pub notify: Notify<PaymentProcessorEvent>,
    pub task_worker_join_handle: Option<task::JoinHandle<()>>,
    pub stealth_scanner_join_handle: task::JoinHandle<anyhow::Result<()>>,
    pub utxo_recovery_join_handle: task::JoinHandle<anyhow::Result<()>>,
    pub account_monitor_join_handle: task::JoinHandle<anyhow::Result<()>>,
}

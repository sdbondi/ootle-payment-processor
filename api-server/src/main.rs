//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

mod api;
mod cli;
mod event;
mod startup;
mod store;
mod wallet;
mod worker;

use cli::Cli;
use tari_ootle_wallet_sdk_services::Shutdown;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder().filter_level(log::LevelFilter::Info).init();
    let cli = Cli::init();

    let mut shutdown = Shutdown::new();

    let mut app = startup::init_app(&cli, shutdown.to_signal()).await?;

    let api_fut = api::start(cli.bind_address, &app, shutdown.to_signal());

    tokio::select! {
        res = api_fut => {
            res?;
        }
        res = tokio::signal::ctrl_c() => {
            res?;
            shutdown.trigger();
            log::info!("Shutdown signal received, shutting down...");
        }
    }

    // Wait for clean shutdown of background tasks
    if let Some(worker) = app.task_worker_join_handle.take() {
        // Ensure channels/notifies are dropped, some services only exit if these are dropped
        drop(app);
        tokio::select! {
           res = worker => {
                res?;
            },
            res = tokio::signal::ctrl_c() => {
                res?;
                log::info!("Forced shutdown signal received, terminating immediately...");
            }
        }
    }

    Ok(())
}

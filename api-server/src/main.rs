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

    let shutdown = Shutdown::new();

    let app = startup::init_app(&cli, shutdown.to_signal()).await?;

    api::start(cli.bind_address, app, shutdown.to_signal()).await?;

    // TODO: handle ctrl-c and shutdown properly

    Ok(())
}

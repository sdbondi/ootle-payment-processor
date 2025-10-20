//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use crate::startup::App;
use std::net::SocketAddr;
use tari_ootle_wallet_sdk_services::ShutdownSignal;
use tokio::net;

mod context;
mod error;
pub mod handlers;
mod routes;

pub async fn start(addr: SocketAddr, app: App, shutdown: ShutdownSignal) -> anyhow::Result<()> {
    let app = routes::create_router(app);
    let listener = net::TcpListener::bind(addr).await?;

    log::info!("🚀 Payment Gateway server listening on http://{}", addr);

    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    Ok(())
}

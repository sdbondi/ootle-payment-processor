//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use clap::Parser;
use std::net::SocketAddr;
use tari_ootle_wallet_sdk::Network;
use url::Url;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, default_value = "0.0.0.0:13000")]
    pub bind_address: SocketAddr,
    #[arg(
        short,
        long,
        default_value = "postgres://postgres:postgres@localhost:5432/ootle-payment",
        env = "DATABASE_URL"
    )]
    pub connection_string: Box<str>,
    #[arg(
        short,
        long,
        default_value = "./data/wallet_sdk_store.sqlite",
        env = "SDK_STORE_PATH"
    )]
    pub sdk_store_path: Box<str>,
    #[arg(short = 'l', long, default_value_t = ([127u8, 0, 0, 1], 13000).into(),  env = "LISTEN_ADDRESS")]
    pub listen_address: SocketAddr,
    #[arg(long, default_value = "http://18.217.22.26:12500/json_rpc", env = "INDEXER_API_URL")]
    pub indexer_api_url: Url,
    #[arg(long, default_value_t = Network::LocalNet, env = "WALLET_NETWORK")]
    pub network: Network,
    /// Run in idle mode (do not process payments)
    #[arg(long, default_value_t = false)]
    pub idle: bool,
}

impl Cli {
    pub fn init() -> Self {
        Self::parse()
    }
}

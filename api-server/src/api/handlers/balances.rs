// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use crate::api::error::ErrorResponse;
use crate::wallet::BalanceEntry;
use axum::response::Json;
use axum::Extension;
use serde::Serialize;
use tari_ootle_common_types::optional::Optional;
use tari_ootle_wallet_sdk::OotleAddress;

#[derive(Serialize)]
pub struct BalancesResponse {
    pub account_address: OotleAddress,
    pub balances: Vec<BalanceEntry>,
}

#[utoipa::path(get, path = "/balances", description = "List balances")]
pub async fn list(Extension(context): Extension<HandlerContext>) -> Result<Json<BalancesResponse>, ErrorResponse> {
    let account = context
        .wallet()
        .get_account_or_default(None)
        .optional()
        .map_err(ErrorResponse::anyhow)?
        .ok_or_else(|| ErrorResponse::not_found("No default account found"))?;

    let balances = context.wallet().get_balances(account.component_address())?;

    Ok(Json(BalancesResponse {
        account_address: account.address,
        balances,
    }))
}

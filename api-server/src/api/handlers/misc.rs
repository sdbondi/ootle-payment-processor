// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use axum::response::Json;
use axum::Extension;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct VersionResponse {
    pub version: String,
}

#[utoipa::path(get, path = "/version", description = "Get API version")]
pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[utoipa::path(get, path = "/health", description = "Get health status")]
pub async fn health(Extension(context): Extension<HandlerContext>) -> Json<HashMap<&'static str, String>> {
    let err = context.store().backend().test_connection().await.err();
    let mut response = HashMap::new();
    if let Some(err) = err {
        response.insert("status", "error".to_string());
        response.insert("database_error", err.to_string());
    }
    if !context.is_worker_running() {
        response.insert("status", "error".to_string());
        response.insert("worker_error", "Worker has stopped running".to_string());
    }
    if !context.is_account_monitor_running() {
        response.insert("status", "error".to_string());
        response.insert(
            "account_monitor_error",
            "Account monitor has stopped running".to_string(),
        );
    }
    if !context.is_utxo_scanner_running() {
        response.insert("status", "error".to_string());
        response.insert("utxo_scanner_error", "Utxo scanner has stopped running".to_string());
    }
    if !context.is_stealth_scanner_running() {
        response.insert("status", "error".to_string());
        response.insert(
            "stealth_scanner_error",
            "Stealth scanner has stopped running".to_string(),
        );
    }

    if response.is_empty() {
        response.insert("status", "ok".to_string());
    }
    Json(response)
}

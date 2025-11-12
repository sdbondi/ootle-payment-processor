// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use axum::response::Json;
use axum::Extension;
use indexmap::IndexMap;
use serde::Serialize;

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
pub async fn health(Extension(context): Extension<HandlerContext>) -> Json<IndexMap<&'static str, String>> {
    let stats = context.store().get_queue_stats().await;
    let mut response = IndexMap::new();
    let mut status = "ok";
    match stats {
        Ok(s) => {
            response.insert("pending_jobs", s.pending_jobs.to_string());
            response.insert("in_progress_jobs", s.in_progress_jobs.to_string());
            response.insert("completed_jobs", s.completed_jobs.to_string());
            response.insert("failed_jobs", s.failed_jobs.to_string());
        },
        Err(err) => {
            response.insert("database_error", err.to_string());
            status = "error";
        },
    }
    if !context.is_worker_running() {
        response.insert("status", "error".to_string());
        response.insert("worker_error", "Worker has stopped running".to_string());
        status = "error";
    }
    if !context.is_account_monitor_running() {
        response.insert("status", "error".to_string());
        response.insert(
            "account_monitor_error",
            "Account monitor has stopped running".to_string(),
        );
        status = "error";
    }

    if !context.is_utxo_scanner_running() {
        response.insert("utxo_scanner_error", "Utxo scanner has stopped running".to_string());
        status = "error";
    }
    if !context.is_stealth_scanner_running() {
        status = "error";
        response.insert(
            "stealth_scanner_error",
            "Stealth scanner has stopped running".to_string(),
        );
    }
    response.insert("status", status.to_string());
    Json(response)
}

// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use crate::api::error::ErrorResponse;
use crate::event::PaymentProcessorEvent;
use crate::worker::jobs::send_payment::SendPaymentJobPayload;
use axum::extract::Path;
use axum::response::Json;
use axum::Extension;
use ootle_payment_processor_storage::models::{JobStatus, JobType};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tari_engine_types::template_lib_models::ResourceAddress;
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::models::WalletTransaction;
use tari_ootle_wallet_sdk::OotleAddress;

#[derive(Deserialize, Serialize)]
pub struct PaymentCreateRequest {
    pub amount: u64,
    pub resource: ResourceAddress,
    pub max_fee: u64,
    pub to_address: OotleAddress,
    pub memo: Option<Memo>,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Serialize)]
pub struct PaymentCreateResponse {
    pub payment_id: uuid::Uuid,
}

#[utoipa::path(post, path = "/payments", description = "Create a new payment")]
pub async fn create(
    Extension(context): Extension<HandlerContext>,
    Json(req): Json<PaymentCreateRequest>,
) -> Result<Json<PaymentCreateResponse>, ErrorResponse> {
    if req.amount == 0 {
        return Err(ErrorResponse::bad_request("Amount must be greater than zero"));
    }

    let id = context
        .store()
        .enqueue_job(
            JobType::ProcessPayment,
            Some(
                serde_json::to_value(&SendPaymentJobPayload {
                    amount: req.amount,
                    resource: req.resource,
                    max_fee: req.max_fee,
                    to_address: req.to_address,
                    memo: req.memo,
                })
                .unwrap(),
            ),
            req.priority,
        )
        .await
        .map_err(ErrorResponse::anyhow)?;

    context.notify(PaymentProcessorEvent::TaskCreated { task_id: id });

    Ok(Json(PaymentCreateResponse { payment_id: id }))
}

#[derive(Serialize)]
pub struct PaymentGetResponse {
    pub payment_id: uuid::Uuid,
    pub status: JobStatus,
    pub job_type: JobType,
    pub exec_time: Option<Duration>,
    pub result: Option<WalletTransaction>,
    pub priority: u32,
    pub failure_reason: Option<Box<str>>,
}

#[utoipa::path(get, path = "/payments/{payment_id}", description = "Get payment")]
pub async fn get(
    Extension(context): Extension<HandlerContext>,
    Path(payment_id): Path<uuid::Uuid>,
) -> Result<Json<PaymentGetResponse>, ErrorResponse> {
    let job = context
        .store()
        .get_job(payment_id)
        .await
        .map_err(ErrorResponse::anyhow)?
        .ok_or_else(|| ErrorResponse::not_found("Payment not found"))?;

    Ok(Json(PaymentGetResponse {
        payment_id: job.id,
        status: job.status,
        job_type: job.job_type,
        exec_time: Some(job.execution_time).filter(|t| t.as_millis() > 0),
        result: job
            .result
            .map(serde_json::from_value)
            .transpose()
            .map_err(ErrorResponse::anyhow)?,
        priority: job.priority,
        failure_reason: job.failure_reason,
    }))
}

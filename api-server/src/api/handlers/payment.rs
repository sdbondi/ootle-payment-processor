// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use crate::api::error::ErrorResponse;
use crate::event::PaymentProcessorEvent;
use crate::worker::jobs::send_payment::SendPaymentJobPayload;
use axum::response::Json;
use axum::Extension;
use ootle_payment_processor_storage::models::TaskType;
use serde::{Deserialize, Serialize};
use tari_engine_types::template_lib_models::ResourceAddress;
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::OotleAddress;

#[derive(Deserialize, Serialize)]
pub struct PaymentCreateRequest {
    pub amount: u64,
    pub resource: ResourceAddress,
    pub max_fee: u64,
    pub to_address: OotleAddress,
    pub memo: Option<Memo>,
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
    let id = context
        .store()
        .push_task(
            TaskType::ProcessPayment,
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
        )
        .await
        .map_err(ErrorResponse::anyhow)?;

    context.notify(PaymentProcessorEvent::TaskCreated { task_id: id });

    Ok(Json(PaymentCreateResponse { payment_id: id }))
}

// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::worker::context::JobContext;
use ootle_payment_processor_storage::WriteableStore;
use serde::{Deserialize, Serialize};
use tari_engine_types::template_lib_models::ResourceAddress;
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::OotleAddress;
use tokio::task::block_in_place;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPaymentJobPayload {
    pub amount: u64,
    pub resource: ResourceAddress,
    pub max_fee: u64,
    pub to_address: OotleAddress,
    pub memo: Option<Memo>,
}

pub async fn do_work<TStore: WriteableStore>(
    context: JobContext<TStore>,
    job_id: uuid::Uuid,
    payload: SendPaymentJobPayload,
) -> anyhow::Result<()> {
    log::info!("Starting SendPayment job: {}", job_id);

    let (transaction, lock_id) = block_in_place(|| {
        let transfer = context.wallet.create_transfer(
            Some("payment-processor"),
            &payload.to_address,
            payload.max_fee,
            payload.amount,
            &[payload.amount],
            payload.memo.as_ref(),
        )?;

        let transaction = context.wallet.create_transfer_transaction(&transfer)?;
        let transaction = context.wallet.sign_transaction(
            transaction,
            transfer.required_signer_key_branch,
            transfer.required_signer_key_id,
        );
        anyhow::Ok((transaction, transfer.lock_id))
    })?;

    let finalized_wallet_tx = context
        .wallet
        .submit_transaction(transaction, None, Some(lock_id))
        .await?;

    log::info!("Submitted payment transaction with ID: {}", finalized_wallet_tx.id);
    if let Some(reason) = finalized_wallet_tx.finalize.as_ref().and_then(|f| f.any_reject()) {
        log::error!("Payment transaction {} failed: {}", finalized_wallet_tx.id, reason);
        return Err(anyhow::anyhow!(
            "Payment transaction {} failed: {}",
            finalized_wallet_tx.id,
            reason
        ));
    }

    log::info!("Completed SendPayment job: {}", job_id);
    Ok(())
}

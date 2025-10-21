// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::wallet::Sdk;
use crate::worker::context::JobContext;
use crate::worker::JobResult;
use anyhow::Error;
use ootle_payment_processor_storage::models::Job;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tari_engine_types::template_lib_models::ResourceAddress;
use tari_engine_types::ToByteType;
use tari_ootle_wallet_sdk::apis::confidential_transfer::ConfidentialTransferInputSelection;
use tari_ootle_wallet_sdk::apis::stealth_transfer::{StealthTransferApiError, StealthTransferParams, TransferOutput};
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::OotleAddress;
use tari_transaction::TransactionSignature;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPaymentJobPayload {
    pub amount: u64,
    pub resource: ResourceAddress,
    pub max_fee: u64,
    pub to_address: OotleAddress,
    pub memo: Option<Memo>,
}

pub async fn do_work(context: JobContext, job: Job, params: SendPaymentJobPayload) -> anyhow::Result<JobResult> {
    log::info!("Starting SendPayment job: {} (attempt {})", job.id, job.attempts + 1);

    let default_account = context.wallet.get_account_or_default(None)?;

    let balances = context.wallet.get_balances(default_account.component_address())?;
    let balance = balances
        .iter()
        .find(|b| b.resource_address == params.resource)
        .ok_or_else(|| anyhow::anyhow!("No balance found for resource {}", params.resource))?;
    if balance.total_balance() < params.amount + params.max_fee {
        let delay = 30 + 30u64.saturating_sub(u64::from(job.priority));
        log::warn!(
            "💸 Insufficient balance for payment. Available: {}, Required: {}. Suspending job for {delay} seconds...",
            balance.total_balance(),
            params.amount + params.max_fee
        );
        return Ok(JobResult::RetryIn {
            duration: Duration::from_secs(delay),
        });
    }

    log::info!(
        "💸 Sufficient balance {}. Creating payment transaction...",
        balance
            .total_balance()
            .to_decimal_string(u32::from(balance.divisibility))
    );

    let sdk = context.wallet.sdk();

    let result = sdk
        .stealth_transfer_api()
        .transfer(
            default_account,
            StealthTransferParams {
                input_selection: ConfidentialTransferInputSelection::PreferRevealed,
                blinded_output_amount: params.amount.into(),
                revealed_output_amount: Default::default(),
                output_memo: params.memo,
                destination_address: params.to_address,
                resource_address: params.resource,
                max_fee: params.max_fee,
                is_dry_run: false,
            },
        )
        .await;

    match result {
        Ok(transfer) => submit_transfer(&context, job, sdk, transfer).await,
        // If several jobs are being processed concurrently, it is possible that the balance check above passes before another transfer uses the funds
        Err(StealthTransferApiError::InsufficientFunds) => {
            log::warn!(
                "💸 Insufficient funds detected during transfer creation. Suspending job {} for 30 seconds...",
                job.id
            );
            Ok(JobResult::RetryIn {
                duration: Duration::from_secs(30 + 30u64.saturating_sub(u64::from(job.priority))),
            })
        },
        Err(e) => {
            log::error!("Failed to create payment transaction: {}", e);
            Err(anyhow::anyhow!("Failed to create payment transaction: {}", e))
        },
    }
}

async fn submit_transfer(
    context: &JobContext,
    job: Job,
    sdk: &Sdk,
    transfer: TransferOutput,
) -> Result<JobResult, Error> {
    let transaction = transfer.transaction.authorized_sealed_signer();
    let main_pk = transfer.main_signer.public_key().to_byte_type();

    // Add additional signature if needed
    let additional_sig = transfer
        .additional_signer
        .as_ref()
        .map(|s| {
            sdk.local_signer_api()
                .get_signature(s.branch, s.key_id, &main_pk, &transaction)
        })
        .transpose()?
        .map(|sig| TransactionSignature::new(sig.public_key.to_byte_type(), sig.signature.to_byte_type()));

    let transaction = transaction.build_with_signatures(additional_sig.into_iter().collect());

    // Sign and seal the final transaction
    let signed_transaction =
        sdk.local_signer_api()
            .sign(transfer.main_signer.branch, transfer.main_signer.key_id, transaction)?;

    let finalized_wallet_tx = sdk.stealth_transfer_api().unlock_on_failure(
        transfer.lock_id,
        context
            .wallet
            .submit_transaction(signed_transaction, None, Some(transfer.lock_id))
            .await,
    )?;

    log::info!("Submitted payment transaction with ID: {}", finalized_wallet_tx.id);
    if let Some(reason) = finalized_wallet_tx.finalize.as_ref().and_then(|f| f.any_reject()) {
        log::error!("Payment transaction {} failed: {}", finalized_wallet_tx.id, reason);
        return Err(anyhow::anyhow!(
            "Payment transaction {} failed: {}",
            finalized_wallet_tx.id,
            reason
        ));
    }

    log::info!("Completed SendPayment job: {}", job.id);
    Ok(JobResult::Completed {
        result: serde_json::json!(finalized_wallet_tx),
    })
}

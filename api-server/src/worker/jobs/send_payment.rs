// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::wallet::Sdk;
use crate::worker::context::JobContext;
use crate::worker::JobResult;
use anyhow::Error;
use ootle_payment_processor_storage::models::{Job, JobStatus};
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use tari_engine_types::template_lib_models::ResourceAddress;
use tari_engine_types::ToByteType;
use tari_ootle_wallet_sdk::apis::confidential_transfer::ConfidentialTransferInputSelection;
use tari_ootle_wallet_sdk::apis::stealth_transfer::{
    StealthTransferApiError, StealthTransferOutput, StealthTransferParams, TransferOutput,
};
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::OotleAddress;
use tari_transaction::TransactionSignature;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPaymentJobPayload {
    pub transfers: Box<[TransferRequest]>,
    pub resource: ResourceAddress,
    pub max_fee: u64,
}

impl SendPaymentJobPayload {
    pub fn total_transfer_amount(&self) -> u64 {
        self.transfers.iter().map(|t| t.amount).sum()
    }

    pub fn total_spend_amount(&self) -> u64 {
        self.total_transfer_amount() + self.max_fee
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransferRequest {
    pub amount: u64,
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
    if balance.total_balance() < params.total_spend_amount() {
        log::warn!(
            "🟡 Insufficient balance for payment. Available: {}, Required: {}. Suspending job until funds are available...",
            balance.total_balance(),
            params.total_spend_amount()
        );
        if matches!(job.status, JobStatus::WaitingForBalance) {
            log::info!("Job {} was already waiting for balance. Retry in >=10 seconds.", job.id);
            return Ok(JobResult::RetryIn {
                duration: std::time::Duration::from_secs(10 + rng().random_range(0u64..=10)),
            });
        }

        return Ok(JobResult::WaitForBalance {
            resource: params.resource,
            amount: params.total_spend_amount(),
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
                outputs: params
                    .transfers
                    .iter()
                    .map(|t| TransferOutput {
                        blinded_amount: t.amount.into(),
                        revealed_amount: Default::default(),
                        memo: t.memo.clone(),
                        address: t.to_address.clone(),
                    })
                    .collect(),
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
                "🟡 Insufficient funds detected during transfer creation. Suspending job {} for 30 seconds...",
                job.id
            );
            Ok(JobResult::WaitForBalance {
                resource: params.resource,
                amount: params.total_spend_amount(),
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
    transfer: StealthTransferOutput,
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
            .submit_transaction(signed_transaction, None, transfer.lock_id)
            .await,
    )?;

    log::info!("Submitted payment transaction with ID: {}", finalized_wallet_tx.id);
    if let Some(reason) = finalized_wallet_tx.finalize.as_ref().and_then(|f| f.reject()) {
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

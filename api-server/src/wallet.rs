// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use anyhow::anyhow;
use log::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tari_engine_types::template_lib_models::{ComponentAddress, ResourceAddress, VaultId};
use tari_ootle_common_types::displayable::Displayable;
use tari_ootle_wallet_sdk::apis::accounts::AccountsApiError;
use tari_ootle_wallet_sdk::cipher_seed::CipherSeedRestore;
use tari_ootle_wallet_sdk::models::{
    AccountWithAddress, NewAccountData, TransactionStatus, WalletLockId, WalletTransaction,
};
use tari_ootle_wallet_sdk::WalletSdk;
use tari_ootle_wallet_sdk_services::indexer_rest_api::IndexerRestApiNetworkInterface;
use tari_ootle_wallet_storage_sqlite::SqliteWalletStore;
use tari_template_lib_types::{Amount, ResourceType};
use tari_transaction::{Transaction, TransactionId};

pub type Sdk = WalletSdk<SqliteWalletStore, IndexerRestApiNetworkInterface>;

#[derive(Debug, Clone)]
pub struct Wallet {
    sdk: Sdk,
}

impl Wallet {
    pub fn new(sdk: Sdk) -> Self {
        Self { sdk }
    }

    pub fn sdk(&self) -> &Sdk {
        &self.sdk
    }

    pub fn get_balances(&self, account_address: &ComponentAddress) -> anyhow::Result<Vec<BalanceEntry>> {
        let sdk = self.sdk();

        let vaults = sdk.accounts_api().get_vaults_by_account(account_address)?;
        let stealth_outputs = sdk
            .stealth_outputs_api()
            .get_unspent_outputs_by_account(account_address)?;

        let mut balances = Vec::with_capacity(vaults.len());
        let mut vaulted_resources = HashSet::new();
        for vault in vaults {
            let (utxo_count, confidential_balance) = if vault.resource_type.is_stealth() {
                let (utxo_count, stealth_balance) = stealth_outputs
                    .iter()
                    .filter(|o| o.owner_account == *account_address && o.resource_address == vault.resource_address)
                    .map(|o| o.value)
                    .fold((0usize, Amount::zero()), |(cnt, acc), o| (cnt + 1, acc + o));

                if stealth_balance.is_positive() {
                    // If the vault exists, we add the confidential balance to this entry and, we don't want to add it again to the balances list for stealth utxos below.
                    vaulted_resources.insert(vault.resource_address);
                }

                (utxo_count, stealth_balance)
            } else {
                (0, vault.confidential_balance)
            };

            balances.push(BalanceEntry {
                vault_address: Some(vault.id),
                resource_address: vault.resource_address,
                balance: vault.available_revealed_balance(),
                resource_type: vault.resource_type,
                confidential_balance,
                num_outputs: utxo_count,
                token_symbol: vault.token_symbol,
                divisibility: vault.divisibility,
            })
        }

        let stealth_outputs_map = stealth_outputs
            .iter()
            .filter(|o| !vaulted_resources.contains(&o.resource_address))
            .fold(HashMap::new(), |mut acc, o| {
                acc.entry(o.resource_address)
                    .and_modify(|(cnt, v)| {
                        *cnt += 1;
                        *v += o.value
                    })
                    .or_insert((1, o.value));
                acc
            });

        let all_resources = sdk.resources_api().get_many(stealth_outputs_map.keys())?;

        for (resource_address, (num_outputs, total_value)) in stealth_outputs_map {
            let resource = all_resources.get(&resource_address);
            balances.push(BalanceEntry {
                vault_address: None,
                resource_address,
                balance: Amount::zero(),
                resource_type: ResourceType::Stealth,
                confidential_balance: total_value,
                num_outputs,
                // It's not guaranteed by the wallet that we know the resource, so instead of erroring, we'll return
                // something
                token_symbol: resource.as_ref().and_then(|r| r.token_symbol()).map(|s| s.to_owned()),
                divisibility: resource.as_ref().map(|r| r.divisibility()).unwrap_or(0),
            });
        }

        Ok(balances)
    }

    pub fn get_account_or_default(&self, name: Option<&str>) -> Result<AccountWithAddress, AccountsApiError> {
        let sdk = self.sdk();

        let account = match name {
            Some(name) => sdk.accounts_api().get_account_by_name(name)?,
            None => sdk.accounts_api().get_default()?,
        };
        Ok(account)
    }

    pub fn create_account(&mut self, name: &str, set_default: bool) -> anyhow::Result<AccountWithAddress> {
        self.sdk
            .initialize_cipher_seed(CipherSeedRestore::CreateNewIfRequired)?;
        let address = self.sdk.key_manager_api().next_account_address()?;
        let account = self
            .sdk
            .accounts_api()
            .create_account(Some(name), set_default, address)?;
        Ok(account)
    }

    // pub fn create_transfer(
    //     &self,
    //     src_account: Option<&str>,
    //     dest_address: &OotleAddress,
    //     fee_amount: u64,
    //     amount: u64,
    //     outputs: &[u64],
    //     memo: Option<&Memo>,
    // ) -> anyhow::Result<TransferOutput> {
    //     assert_eq!(
    //         outputs.iter().sum::<u64>(),
    //         amount,
    //         "Outputs do not sum to input amount"
    //     );
    //     let src_account = match src_account {
    //         Some(name) => self.sdk().accounts_api().get_account_by_name(name).unwrap(),
    //         None => self.sdk().accounts_api().get_default().unwrap(),
    //     };
    //     let spend_key_id = src_account
    //         .owner_key_id()
    //         .ok_or_else(|| anyhow::anyhow!("Source account does not have the required spend key"))?;
    //     let view_key_id = src_account.view_only_key_id();
    //
    //     let outputs_api = self.sdk().stealth_outputs_api();
    //
    //     let lock_id = outputs_api.create_lock()?;
    //     let inputs_to_spend = self.sdk().stealth_transfer_api().lock_inputs_for_transfer(
    //         lock_id,
    //         src_account.component_address(),
    //         XTR,
    //         (amount + fee_amount).into(),
    //         ConfidentialTransferInputSelection::PreferRevealed,
    //     )?;
    //
    //     let dest_address = dest_address
    //         .try_from_byte_type()
    //         .map_err(|err| anyhow!("Destination address is not a Ristretto Ootle address: {err}"))?;
    //
    //     let src_address = src_account.address.try_from_byte_type()?;
    //     let spends_revealed_funds = inputs_to_spend.revealed.is_positive();
    //
    //     let change_output = Some(OutputToCreate {
    //         owner_address: &src_address,
    //         amount: inputs_to_spend.total_amount() - Amount::from(amount) - Amount::from(fee_amount),
    //         memo: None,
    //     })
    //     .filter(|o| o.amount.is_positive());
    //
    //     let transfer_outputs = outputs.iter().map(|&amt| OutputToCreate {
    //         owner_address: &dest_address,
    //         amount: amt.into(),
    //         memo,
    //     });
    //
    //     let (key_branch, key_id, public_key) = if spends_revealed_funds {
    //         (
    //             KeyBranch::Account,
    //             src_account
    //                 .owner_key_id()
    //                 .expect("Source account does not have the required spend key"),
    //             *src_account.owner_public_key(),
    //         )
    //     } else {
    //         // If we are not spending revealed funds, we can use the nonce key for signing
    //         let nonce_key = self.sdk.key_manager_api().next_public_key(KeyBranch::Nonce)?;
    //         (KeyBranch::Nonce, nonce_key.key_id, nonce_key.public_key.to_byte_type())
    //     };
    //
    //     let params = TransferStatementParams {
    //         spend_key_branch: KeyBranch::Account,
    //         spend_key_id,
    //         view_only_key_id: view_key_id,
    //         resource_address: &XTR,
    //         resource_view_key: None,
    //         inputs: &inputs_to_spend.inputs,
    //         input_revealed_amount: inputs_to_spend.revealed,
    //         outputs: transfer_outputs.chain(change_output),
    //         // TODO: this only works with XTR
    //         output_revealed_amount: fee_amount.into(),
    //         required_signer: public_key,
    //     };
    //
    //     let transfer = outputs_api.generate_transfer_statement(params)?;
    //
    //     Ok(TransferOutput {
    //         statement: transfer,
    //         resource_address: XTR,
    //         lock_id,
    //         required_signer_key_branch: key_branch,
    //         required_signer_key_id: key_id,
    //         account_component_address: *src_account.component_address(),
    //     })
    // }
    //
    // pub fn sign_transaction(
    //     &self,
    //     transaction: UnsignedTransaction,
    //     key_branch: KeyBranch,
    //     key_id: KeyId,
    // ) -> Transaction {
    //     self.sdk
    //         .local_signer_api()
    //         .sign(key_branch, key_id, transaction.authorized_sealed_signer().build())
    //         .unwrap()
    // }
    //
    // pub fn create_transfer_transaction(&self, transfer: &TransferOutput) -> anyhow::Result<UnsignedTransaction> {
    //     let utxo_inputs = transfer
    //         .statement
    //         .inputs_statement
    //         .inputs
    //         .iter()
    //         .map(|i| UtxoAddress::new(transfer.resource_address, i.commitment.into()))
    //         .map(SubstateRequirement::unversioned);
    //
    //     let revealed_input_amount = transfer.statement.inputs_statement.revealed_amount;
    //     let statement = &transfer.statement;
    //
    //     let maybe_account_input = revealed_input_amount
    //         .is_positive()
    //         .then_some(transfer.account_component_address);
    //     let maybe_vault_input = maybe_account_input
    //         .as_ref()
    //         .and_then(|_| {
    //             self.sdk()
    //                 .accounts_api()
    //                 .get_vault_by_resource(&transfer.account_component_address, &XTR)
    //                 .optional()
    //                 .transpose()
    //         })
    //         .transpose()?;
    //
    //     let transaction = Transaction::builder()
    //         .for_network(self.network().as_byte())
    //         .with_fee_instructions_builder(|builder| {
    //             if revealed_input_amount.is_positive() {
    //                 builder
    //                     .call_method(
    //                         transfer.account_component_address,
    //                         "withdraw",
    //                         args![XTR, revealed_input_amount],
    //                     )
    //                     .put_last_instruction_output_on_workspace("fee_input_bucket")
    //                     .pay_fee_stealth_with_input_bucket(statement.clone(), "fee_input_bucket")
    //             } else {
    //                 builder.pay_fee_stealth(statement.clone())
    //             }
    //         })
    //         .with_inputs(utxo_inputs)
    //         .with_inputs(maybe_account_input.map(Into::into))
    //         .with_inputs(maybe_vault_input.map(|v| v.id.into()))
    //         // TODO: remove the need to add this input
    //         .add_input(XTR)
    //         .build_unsigned_transaction();
    //     Ok(transaction)
    // }

    pub async fn submit_transaction(
        &self,
        transaction: Transaction,
        new_account_data: Option<NewAccountData>,
        lock_id: Option<WalletLockId>,
    ) -> anyhow::Result<WalletTransaction> {
        let id = self
            .sdk
            .transaction_api()
            .insert_new_transaction(transaction, new_account_data, false)?;
        if let Some(lock_id) = lock_id {
            self.sdk.stealth_outputs_api().locks_set_transaction_id(lock_id, id)?;
        }
        if !self.sdk.transaction_api().submit_transaction(id).await? {
            return Err(anyhow!("Failed to submit transaction {}", id));
        }

        self.wait_for_transaction_to_finalize(id).await
    }

    async fn wait_for_transaction_to_finalize(&self, id: TransactionId) -> anyhow::Result<WalletTransaction> {
        loop {
            let maybe_tx = self
                .sdk()
                .transaction_api()
                .check_and_store_finalized_transaction(id)
                .await?;
            match maybe_tx {
                Some(tx) => {
                    return if matches!(tx.status, TransactionStatus::Accepted) {
                        info!("Transaction {} was accepted", id);
                        Ok(tx)
                    } else {
                        Err(anyhow!(
                            "Transaction {} failed: {:?} {} {}",
                            id,
                            tx.status,
                            tx.invalid_reason.as_deref().unwrap_or(""),
                            tx.finalize.as_ref().and_then(|f| f.result.any_reject()).display()
                        ))
                    };
                },
                None => {
                    info!("Transaction {} is still pending...", id);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                },
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceEntry {
    pub vault_address: Option<VaultId>,
    pub resource_address: ResourceAddress,
    pub balance: Amount,
    pub resource_type: ResourceType,
    pub num_outputs: usize,
    pub confidential_balance: Amount,
    pub token_symbol: Option<String>,
    pub divisibility: u8,
}

impl BalanceEntry {
    pub fn total_balance(&self) -> Amount {
        self.balance + self.confidential_balance
    }
}

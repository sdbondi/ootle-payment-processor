// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use anyhow::anyhow;
use log::*;
use tari_engine_types::template_lib_models::{
    ComponentAddress, ResourceAddress, StealthTransferStatement, UtxoAddress,
};
use tari_engine_types::{FromByteType, ToByteType};
use tari_ootle_common_types::displayable::Displayable;
use tari_ootle_common_types::optional::Optional;
use tari_ootle_common_types::{Network, SubstateRequirement};
use tari_ootle_wallet_sdk::apis::accounts::AccountsApiError;
use tari_ootle_wallet_sdk::apis::confidential_transfer::ConfidentialTransferInputSelection;
use tari_ootle_wallet_sdk::apis::stealth_outputs::TransferStatementParams;
use tari_ootle_wallet_sdk::apis::stealth_transfer::OutputToCreate;
use tari_ootle_wallet_sdk::cipher_seed::CipherSeedRestore;
use tari_ootle_wallet_sdk::constants::{XTR, XTR_FAUCET_COMPONENT_ADDRESS, XTR_FAUCET_VAULT_ADDRESS};
use tari_ootle_wallet_sdk::crypto::memo::Memo;
use tari_ootle_wallet_sdk::models::{
    AccountWithAddress, KeyBranch, KeyId, NewAccountData, TransactionStatus, WalletLockId, WalletTransaction,
};
use tari_ootle_wallet_sdk::network::WalletNetworkInterface;
use tari_ootle_wallet_sdk::{OotleAddress, WalletSdk};
use tari_ootle_wallet_sdk_services::indexer_rest_api::IndexerRestApiNetworkInterface;
use tari_ootle_wallet_storage_sqlite::SqliteWalletStore;
use tari_template_lib_types::Amount;
use tari_transaction::{args, Transaction, TransactionId, UnsignedTransaction};

pub type Sdk = WalletSdk<SqliteWalletStore, IndexerRestApiNetworkInterface>;

#[derive(Debug, Clone)]
pub struct Wallet {
    sdk: Sdk,
}

impl Wallet {
    pub fn new(sdk: Sdk) -> Self {
        Self { sdk }
    }

    pub fn network(&self) -> Network {
        self.sdk.network()
    }

    pub fn sdk(&self) -> &Sdk {
        &self.sdk
    }

    pub fn get_account_or_default(&self, name: Option<&str>) -> Result<AccountWithAddress, AccountsApiError> {
        let sdk = self.sdk();

        let account = match name {
            Some(name) => sdk.accounts_api().get_account_by_name(name)?,
            None => sdk.accounts_api().get_default()?,
        };
        Ok(account)
    }

    pub async fn check_indexer_connection(&self) -> anyhow::Result<()> {
        self.sdk()
            .get_network_interface()
            .wait_until_ready()
            .await
            .map_err(|e| anyhow!("Failed to connect to indexer: {}", e))
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

    pub async fn request_testnet_faucet_coins<T: Into<Amount>>(
        &self,
        address: &ComponentAddress,
        amount: T,
    ) -> anyhow::Result<()> {
        let sdk = self.sdk();
        let accounts_api = sdk.accounts_api();

        let account = accounts_api.get_account_by_address(address)?;
        const FEE: u64 = 1_000;
        let amount = amount.into();

        let account_owner_key_id = account
            .owner_key_id()
            .ok_or_else(|| anyhow!("cannot create free test coins for an account without an owner key"))?;

        info!(
            "💰️ Creating free test coins for account: {}",
            account.account.component_address,
        );

        let mut inputs = vec![
            SubstateRequirement::unversioned(XTR),
            SubstateRequirement::unversioned(XTR_FAUCET_COMPONENT_ADDRESS),
            SubstateRequirement::unversioned(XTR_FAUCET_VAULT_ADDRESS),
        ];

        if account.is_confirmed_on_chain() {
            info!(
                "💰️ create free test coins: Account {} is on-chain",
                account.account.component_address
            );
            // Add account inputs
            let account_substate = sdk
                .substate_api()
                .get_substate(&account.account.component_address.into())?;
            inputs.push(account_substate.substate_id.into());

            // Add all versioned account child addresses as inputs
            let child_addresses = sdk
                .substate_api()
                .load_dependent_substates(&[&account.account.component_address.into()])?;
            info!(
                "💰️ create free test coins: Loaded {} vaults for existing account: {}",
                child_addresses.len(),
                account
            );
            inputs.extend(child_addresses);
        } else {
            info!(
                "💰️ create free test coins: Account {} is not on-chain, Will create it",
                account.account.component_address
            );
        }

        let transaction = Transaction::builder()
            .for_network(sdk.network().as_byte())
            .with_fee_instructions_builder(|fee_builder| {
                fee_builder
                    .call_method(XTR_FAUCET_COMPONENT_ADDRESS, "take", args![amount])
                    .put_last_instruction_output_on_workspace("faucet_funds")
                    .then(|builder| {
                        if account.is_confirmed_on_chain() {
                            builder.call_method(
                                *account.component_address(),
                                "deposit",
                                args![Workspace("faucet_funds")],
                            )
                        } else {
                            // If the account is not on-chain yet, we create it
                            builder.create_account_with_bucket(*account.address.account_public_key(), "faucet_funds")
                        }
                    })
                    .call_method(*account.component_address(), "pay_fee", args![FEE])
            })
            .with_inputs(inputs.into_iter().map(|input| input.into_unversioned()))
            .build();

        let transaction = sdk
            .local_signer_api()
            .sign(KeyBranch::Account, account_owner_key_id, transaction)?;

        info!(
            "💰️ create free test coins: Submitting transaction {} for account: {}",
            transaction.calculate_id(),
            account.account,
        );

        self.submit_transaction(
            transaction,
            (!account.is_confirmed_on_chain()).then(|| NewAccountData {
                address: *account.component_address(),
            }),
            None,
        )
        .await?;
        Ok(())
    }

    pub fn create_transfer(
        &self,
        src_account: Option<&str>,
        dest_address: &OotleAddress,
        fee_amount: u64,
        amount: u64,
        outputs: &[u64],
        memo: Option<&Memo>,
    ) -> anyhow::Result<TransferOutput> {
        assert_eq!(
            outputs.iter().sum::<u64>(),
            amount,
            "Outputs do not sum to input amount"
        );
        let src_account = match src_account {
            Some(name) => self.sdk().accounts_api().get_account_by_name(name).unwrap(),
            None => self.sdk().accounts_api().get_default().unwrap(),
        };
        let spend_key_id = src_account
            .owner_key_id()
            .ok_or_else(|| anyhow::anyhow!("Source account does not have the required spend key"))?;
        let view_key_id = src_account.view_only_key_id();

        let outputs_api = self.sdk().stealth_outputs_api();

        let lock_id = outputs_api.create_lock()?;
        let inputs_to_spend = self.sdk().stealth_transfer_api().lock_inputs_for_transfer(
            lock_id,
            src_account.component_address(),
            XTR,
            (amount + fee_amount).into(),
            ConfidentialTransferInputSelection::PreferRevealed,
        )?;

        let dest_address = dest_address
            .try_from_byte_type()
            .map_err(|err| anyhow!("Destination address is not a Ristretto Ootle address: {err}"))?;

        let src_address = src_account.address.try_from_byte_type()?;
        let spends_revealed_funds = inputs_to_spend.revealed.is_positive();

        let ch_memo = Memo::new_message("Change").unwrap();
        let change_output = Some(OutputToCreate {
            owner_address: &src_address,
            amount: inputs_to_spend.total_amount() - Amount::from(amount) - Amount::from(fee_amount),
            memo: Some(&ch_memo),
        })
        .filter(|o| o.amount.is_positive());

        let transfer_outputs = outputs.iter().map(|&amt| OutputToCreate {
            owner_address: &dest_address,
            amount: amt.into(),
            memo,
        });

        let (key_branch, key_id, public_key) = if spends_revealed_funds {
            (
                KeyBranch::Account,
                src_account
                    .owner_key_id()
                    .expect("Source account does not have the required spend key"),
                *src_account.owner_public_key(),
            )
        } else {
            // If we are not spending revealed funds, we can use the nonce key for signing
            let nonce_key = self.sdk.key_manager_api().next_public_key(KeyBranch::Nonce)?;

            (KeyBranch::Nonce, nonce_key.key_id, nonce_key.public_key.to_byte_type())
        };

        let params = TransferStatementParams {
            spend_key_branch: KeyBranch::Account,
            spend_key_id,
            view_only_key_id: view_key_id,
            resource_address: &XTR,
            resource_view_key: None,
            inputs: &inputs_to_spend.inputs,
            input_revealed_amount: inputs_to_spend.revealed,
            outputs: transfer_outputs.chain(change_output),
            // TODO: this only works with XTR
            output_revealed_amount: fee_amount.into(),
            required_signer: public_key,
        };

        let transfer = outputs_api.generate_transfer_statement(params)?;

        Ok(TransferOutput {
            statement: transfer,
            resource_address: XTR,
            lock_id,
            required_signer_key_branch: key_branch,
            required_signer_key_id: key_id,
            account_component_address: *src_account.component_address(),
        })
    }

    pub fn sign_transaction(
        &self,
        transaction: UnsignedTransaction,
        key_branch: KeyBranch,
        key_id: KeyId,
    ) -> Transaction {
        self.sdk
            .local_signer_api()
            .sign(key_branch, key_id, transaction.authorized_sealed_signer().build())
            .unwrap()
    }

    pub fn create_transfer_transaction(&self, transfer: &TransferOutput) -> anyhow::Result<UnsignedTransaction> {
        let utxo_inputs = transfer
            .statement
            .inputs_statement
            .inputs
            .iter()
            .map(|i| UtxoAddress::new(transfer.resource_address, i.commitment.into()))
            .map(SubstateRequirement::unversioned);

        let revealed_input_amount = transfer.statement.inputs_statement.revealed_amount;
        let statement = &transfer.statement;

        let maybe_account_input = revealed_input_amount
            .is_positive()
            .then_some(transfer.account_component_address);
        let maybe_vault_input = maybe_account_input
            .as_ref()
            .and_then(|_| {
                self.sdk()
                    .accounts_api()
                    .get_vault_by_resource(&transfer.account_component_address, &XTR)
                    .optional()
                    .transpose()
            })
            .transpose()?;

        let transaction = Transaction::builder()
            .for_network(self.network().as_byte())
            .with_fee_instructions_builder(|builder| {
                if revealed_input_amount.is_positive() {
                    builder
                        .call_method(
                            transfer.account_component_address,
                            "withdraw",
                            args![XTR, statement.inputs_statement.revealed_amount],
                        )
                        .put_last_instruction_output_on_workspace("fee_input_bucket")
                        .pay_fee_stealth_with_input_bucket(statement.clone(), "fee_input_bucket")
                } else {
                    builder.pay_fee_stealth(statement.clone())
                }
            })
            .with_inputs(utxo_inputs)
            .with_inputs(maybe_account_input.map(Into::into))
            .with_inputs(maybe_vault_input.map(|v| v.id.into()))
            // TODO: remove the need to add this input
            .add_input(XTR)
            .build_unsigned_transaction();
        Ok(transaction)
    }

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
                    if matches!(tx.status, TransactionStatus::Accepted) {
                        info!("Transaction {} was accepted", id);
                        return Ok(tx);
                    } else {
                        return Err(anyhow!(
                            "Transaction {} failed: {:?} {} {}",
                            id,
                            tx.status,
                            tx.invalid_reason.as_deref().unwrap_or(""),
                            tx.finalize.as_ref().and_then(|f| f.result.any_reject()).display()
                        ));
                    }
                },
                None => {
                    info!("Transaction {} is still pending...", id);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                },
            }
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TransferOutput {
    pub statement: StealthTransferStatement,
    pub lock_id: WalletLockId,
    pub resource_address: ResourceAddress,
    pub account_component_address: ComponentAddress,
    pub required_signer_key_branch: KeyBranch,
    pub required_signer_key_id: KeyId,
}

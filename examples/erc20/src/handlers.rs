//! ERC-20 Transfer handling and entity updates.

use std::{future::IntoFuture, str::FromStr, sync::Arc, time::Duration};

use alloy_primitives::{Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_sol_types::SolCall;
use async_trait::async_trait;
use bigdecimal::{
    BigDecimal,
    num_bigint::{BigInt, Sign},
};
use raven_engine::{EngineError, EntityStore, EntityStoreExt, Handler, RavenError, RavenResult};
use raven_evm::Parsed;

use crate::{
    ExampleError, Transfer,
    entities::{Account, ERC20Balance, ERC20Contract, ERC20Transfer, Transaction},
    events::IERC20Metadata,
};

pub struct TransferHandler<P = RootProvider> {
    provider: Arc<P>,
    request_timeout: Duration,
}

impl<P: Provider> TransferHandler<P> {
    /// Creates a transfer handler using `provider` for block-bound metadata reads.
    pub fn new(provider: Arc<P>, request_timeout: Duration) -> Self {
        Self {
            provider,
            request_timeout,
        }
    }

    /// Calls one ERC-20 metadata method at `block_hash`, treating reverts as absent data.
    async fn metadata<C: SolCall>(
        &self,
        token: Address,
        block_hash: B256,
        call: C,
    ) -> RavenResult<Option<C::Return>> {
        let transaction = TransactionRequest::default()
            .to(token)
            .input(call.abi_encode().into());
        let result = tokio::time::timeout(
            self.request_timeout,
            self.provider
                .call(transaction)
                .block(block_hash.into())
                .into_future(),
        )
        .await
        .map_err(|_| ExampleError::RequestTimeout)?;
        match result {
            // Empty or undecodable metadata is handled as unavailable contract data.
            Ok(output) => Ok(C::abi_decode_returns_validate(&output).ok()),
            Err(error)
                if error.as_error_resp().is_some_and(|response| {
                    response.code == 3 || response.message.to_lowercase().contains("revert")
                }) =>
            {
                Ok(None)
            }
            // Transport, archive-state and rate-limit failures must not become metadata defaults.
            Err(error) => Err(RavenError::Source(Box::new(error))),
        }
    }

    /// Loads or creates the contract entity for the transfer's token address.
    async fn contract(
        &self,
        entities: &mut dyn EntityStore,
        event: &Parsed<Transfer>,
        token: Address,
    ) -> RavenResult<ERC20Contract> {
        let id = address_id(token);
        if let Some(contract) = entities.load::<ERC20Contract>(&id).await? {
            return Ok(contract);
        }
        // Resolve metadata only at the first indexed Transfer, bound to its exact block.
        let name = self
            .metadata(token, event.block_hash, IERC20Metadata::nameCall)
            .await?;
        let symbol = self
            .metadata(token, event.block_hash, IERC20Metadata::symbolCall)
            .await?;
        let decimals = self
            .metadata(token, event.block_hash, IERC20Metadata::decimalsCall)
            .await?
            .unwrap_or(18);
        let contract = ERC20Contract {
            as_account: id.clone(),
            total_supply: balance_id(&id, None),
            id,
            name,
            symbol,
            decimals,
        };
        save_contract(entities, &contract).await?;
        Ok(contract)
    }

    /// Loads the transfer timestamp, reusing an existing transaction when available.
    async fn timestamp(
        &self,
        entities: &mut dyn EntityStore,
        event: &Parsed<Transfer>,
        transaction_id: &str,
    ) -> RavenResult<u64> {
        if let Some(transaction) = entities.load::<Transaction>(transaction_id).await? {
            if transaction.id != transaction_id
                || transaction.block_number != event.block_number.to_string()
            {
                return Err(ExampleError::InvalidEntityState("Transaction").into());
            }
            return transaction
                .timestamp
                .parse()
                .map_err(|_| ExampleError::InvalidEntityState("Transaction").into());
        }
        let block = tokio::time::timeout(
            self.request_timeout,
            self.provider
                .get_block_by_hash(event.block_hash)
                .into_future(),
        )
        .await
        .map_err(|_| ExampleError::RequestTimeout)?
        .map_err(|error| RavenError::Source(Box::new(error)))?
        .ok_or(EngineError::MissingBlock)?;
        if block.header.hash != event.block_hash || block.header.inner.number != event.block_number
        {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(block.header.inner.timestamp)
    }
}

#[async_trait]
impl<P: Provider> Handler<Parsed<Transfer>> for TransferHandler<P> {
    /// Applies one parsed Transfer after requiring its canonical log metadata.
    async fn handle(
        &self,
        entities: &mut dyn EntityStore,
        event: &Parsed<Transfer>,
    ) -> RavenResult<()> {
        let token = event.address.ok_or(ExampleError::MissingMetadata)?;
        let transaction_id = event
            .transaction_hash
            .ok_or(ExampleError::MissingMetadata)?
            .to_string();
        event.log_index.ok_or(ExampleError::MissingMetadata)?;
        let contract = self.contract(entities, event, token).await?;
        let timestamp = self.timestamp(entities, event, &transaction_id).await?;
        apply_transfer(entities, event, &contract, timestamp).await
    }
}

/// Persists a newly observed contract and its account and total-supply entities.
async fn save_contract(
    entities: &mut dyn EntityStore,
    contract: &ERC20Contract,
) -> RavenResult<()> {
    entities.save(contract).await?;
    let mut account = fetch_account(entities, &contract.as_account).await?;
    account.as_erc20 = Some(contract.id.clone());
    entities.save(&account).await?;
    change_balance(entities, contract, None, &BigInt::from(0)).await?;
    Ok(())
}

/// Writes the transaction and transfer entities, then updates affected balances.
async fn apply_transfer(
    entities: &mut dyn EntityStore,
    event: &Parsed<Transfer>,
    contract: &ERC20Contract,
    timestamp: u64,
) -> RavenResult<()> {
    let transaction_id = event
        .transaction_hash
        .ok_or(ExampleError::MissingMetadata)?
        .to_string();
    let log_index = event.log_index.ok_or(ExampleError::MissingMetadata)?;
    let transaction = Transaction {
        id: transaction_id.clone(),
        timestamp: timestamp.to_string(),
        block_number: event.block_number.to_string(),
    };
    entities.save(&transaction).await?;
    let amount = BigInt::from_bytes_be(Sign::Plus, &event.value.value.to_be_bytes::<32>());
    let mut transfer = ERC20Transfer {
        id: format!("{}-{log_index}", event.block_number),
        emitter: contract.id.clone(),
        transaction: transaction_id,
        timestamp: timestamp.to_string(),
        contract: contract.id.clone(),
        from: None,
        from_balance: None,
        to: None,
        to_balance: None,
        value: decimal_value(&amount, contract.decimals),
        value_exact: amount.to_string(),
    };

    if event.value.from == Address::ZERO {
        change_balance(entities, contract, None, &amount).await?;
    } else {
        let id = address_id(event.value.from);
        fetch_account(entities, &id).await?;
        transfer.from_balance =
            Some(change_balance(entities, contract, Some(&id), &-&amount).await?);
        transfer.from = Some(id);
    }
    if event.value.to == Address::ZERO {
        change_balance(entities, contract, None, &-&amount).await?;
    } else {
        let id = address_id(event.value.to);
        fetch_account(entities, &id).await?;
        transfer.to_balance = Some(change_balance(entities, contract, Some(&id), &amount).await?);
        transfer.to = Some(id);
    }
    // Self-transfers and zero-value transfers still create records and retain balances.
    entities.save(&transfer).await
}

/// Loads an account or creates an empty account entity with `id`.
async fn fetch_account(entities: &mut dyn EntityStore, id: &str) -> RavenResult<Account> {
    if let Some(account) = entities.load::<Account>(id).await? {
        return Ok(account);
    }
    let account = Account {
        id: id.to_owned(),
        as_erc20: None,
    };
    entities.save(&account).await?;
    Ok(account)
}

/// Applies `delta` to one account or total-supply balance and returns its ID.
async fn change_balance(
    entities: &mut dyn EntityStore,
    contract: &ERC20Contract,
    account: Option<&str>,
    delta: &BigInt,
) -> RavenResult<String> {
    let id = balance_id(&contract.id, account);
    let mut balance = entities
        .load::<ERC20Balance>(&id)
        .await?
        .unwrap_or_else(|| ERC20Balance {
            id: id.clone(),
            contract: contract.id.clone(),
            account: account.map(str::to_owned),
            value: "0".to_owned(),
            value_exact: "0".to_owned(),
        });
    if balance.id != id || balance.contract != contract.id || balance.account.as_deref() != account
    {
        return Err(ExampleError::InvalidEntityState("ERC20Balance").into());
    }
    let value = BigInt::from_str(&balance.value_exact)
        .map_err(|_| ExampleError::InvalidEntityState("ERC20Balance"))?
        + delta;
    balance.value = decimal_value(&value, contract.decimals);
    balance.value_exact = value.to_string();
    entities.save(&balance).await?;
    Ok(id)
}

/// Formats an address as the lower-case entity ID.
fn address_id(address: Address) -> String {
    address.to_string().to_lowercase()
}

/// Builds the account or total-supply balance ID for `contract`.
fn balance_id(contract: &str, account: Option<&str>) -> String {
    format!("{contract}/{}", account.unwrap_or("totalSupply"))
}

/// Formats an exact token integer using its decimal precision and 34 significant digits.
fn decimal_value(value: &BigInt, decimals: u8) -> String {
    if value == &BigInt::from(0) {
        return "0".to_owned();
    }
    // Round the integer value to 34 significant digits before applying token decimals.
    // Division by 10^decimals only shifts the exponent after that normalization.
    let rounded = BigDecimal::new(value.clone(), 0).with_prec(34);
    let (digits, scale) = rounded.as_bigint_and_exponent();
    let text = BigDecimal::new(digits, scale + i64::from(decimals)).to_plain_string();
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

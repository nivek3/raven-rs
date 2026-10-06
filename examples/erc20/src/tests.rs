use super::*;
use std::collections::BTreeMap;

use alloy_primitives::{Bytes, U256};
use alloy_provider::ProviderBuilder;
use alloy_transport::mock::Asserter;
use raven_engine::EntityValue;
use serde_json::json;

#[derive(Default)]
struct MemoryEntities(BTreeMap<(String, String), EntityValue>);

impl MemoryEntities {
    /// Returns a stored fixture entity by type and ID.
    fn entity(&self, kind: &str, id: &str) -> &EntityValue {
        &self.0[&(kind.to_owned(), id.to_owned())]
    }
}

#[async_trait]
impl EntityStore for MemoryEntities {
    /// Reads a cloned fixture entity by type and ID.
    async fn get(&mut self, kind: &str, id: &str) -> RavenResult<Option<EntityValue>> {
        Ok(self.0.get(&(kind.to_owned(), id.to_owned())).cloned())
    }

    /// Inserts or replaces one fixture entity.
    async fn put(&mut self, kind: &str, id: &str, data: &EntityValue) -> RavenResult<()> {
        self.0
            .insert((kind.to_owned(), id.to_owned()), data.clone());
        Ok(())
    }

    /// Removes one fixture entity when it exists.
    async fn delete(&mut self, kind: &str, id: &str) -> RavenResult<()> {
        self.0.remove(&(kind.to_owned(), id.to_owned()));
        Ok(())
    }
}

/// Creates a deterministic ERC-20 contract fixture.
fn contract(decimals: u8) -> ERC20Contract {
    let id = address_id(Address::repeat_byte(0xaa));
    ERC20Contract {
        as_account: id.clone(),
        total_supply: balance_id(&id, None),
        id,
        name: Some("Raven Test Token".to_owned()),
        symbol: Some("RAVEN".to_owned()),
        decimals,
    }
}

/// Creates a parsed Transfer fixture with canonical log metadata.
fn event(
    block_number: u64,
    log_index: u64,
    from: Address,
    to: Address,
    amount: U256,
) -> ParsedLog<Transfer> {
    ParsedLog {
        value: Transfer {
            from,
            to,
            value: amount,
        },
        block_number,
        block_hash: B256::repeat_byte(block_number as u8),
        transaction_hash: B256::repeat_byte(block_number as u8 + 100),
        transaction_index: 0,
        log_index,
        address: Address::repeat_byte(0xaa),
    }
}

/// Asserts the persisted exact and decimal balance for one account or total supply.
fn assert_balance(
    entities: &MemoryEntities,
    contract: &ERC20Contract,
    account: Option<Address>,
    exact: &str,
    value: &str,
) {
    let account = account.map(address_id);
    let id = balance_id(&contract.id, account.as_deref());
    assert_eq!(
        entities.entity("ERC20Balance", &id),
        &json!({
            "id": id,
            "contract": contract.id,
            "account": account,
            "value": value,
            "valueExact": exact,
        })
    );
}

#[tokio::test]
/// Verifies entity IDs and fields produced for a standard transfer.
async fn transfer_results_match_openzeppelin_entity_fields_and_ids() {
    let mut entities = MemoryEntities::default();
    let contract = contract(2);
    save_contract(&mut entities, &contract).await.unwrap();
    let a = Address::repeat_byte(1);
    let b = Address::repeat_byte(2);
    let c = Address::repeat_byte(3);
    let transfers = [
        (Address::ZERO, a, 1000),
        (a, b, 300),
        (b, c, 120),
        (c, Address::ZERO, 50),
        (Address::ZERO, b, 25),
        (b, b, 10),
    ];
    for (index, (from, to, amount)) in transfers.into_iter().enumerate() {
        let block = index as u64 + 1;
        apply_transfer(
            &mut entities,
            &event(block, 0, from, to, U256::from(amount)),
            &contract,
            1000 + block,
        )
        .await
        .unwrap();
    }
    assert_eq!(
        entities.entity("ERC20Contract", &contract.id),
        &json!({
            "id": contract.id, "asAccount": contract.id, "name": "Raven Test Token",
            "symbol": "RAVEN", "decimals": 2, "totalSupply": contract.total_supply,
        })
    );
    assert_eq!(
        entities.entity("Account", &contract.id),
        &json!({
            "id": contract.id, "asERC20": contract.id,
        })
    );
    assert_balance(&entities, &contract, Some(a), "700", "7");
    assert_balance(&entities, &contract, Some(b), "205", "2.05");
    assert_balance(&entities, &contract, Some(c), "70", "0.7");
    assert_balance(&entities, &contract, None, "975", "9.75");

    let account = address_id(b);
    let balance = balance_id(&contract.id, Some(&account));
    let tx = B256::repeat_byte(106).to_string();
    assert_eq!(
        entities.entity("ERC20Transfer", "6-0"),
        &json!({
            "id": "6-0", "emitter": contract.id, "contract": contract.id,
            "transaction": tx, "timestamp": "1006", "from": account, "to": account,
            "fromBalance": balance, "toBalance": balance, "value": "0.1", "valueExact": "10",
        })
    );
    assert_eq!(
        entities.entity("Transaction", &tx),
        &json!({
            "id": tx, "timestamp": "1006", "blockNumber": "6",
        })
    );
    assert_eq!(
        entities.entity("ERC20Transfer", "1-0")["from"],
        EntityValue::Null
    );
    assert_eq!(
        entities.entity("ERC20Transfer", "1-0")["fromBalance"],
        EntityValue::Null
    );
    assert_eq!(
        entities.entity("ERC20Transfer", "4-0")["to"],
        EntityValue::Null
    );
    assert!(
        !entities
            .0
            .contains_key(&("Account".to_owned(), address_id(Address::ZERO)))
    );
}

#[tokio::test]
/// Verifies that zero-value transfers and zero balances remain stored.
async fn zero_balances_and_zero_transfers_are_retained_with_shared_transaction_ids() {
    let mut entities = MemoryEntities::default();
    let contract = contract(18);
    save_contract(&mut entities, &contract).await.unwrap();
    let a = Address::repeat_byte(1);
    let b = Address::repeat_byte(2);
    for (index, (from, to, amount)) in [
        (Address::ZERO, a, 100),
        (a, b, 100),
        (b, b, 0),
        (Address::ZERO, Address::ZERO, 0),
    ]
    .into_iter()
    .enumerate()
    {
        apply_transfer(
            &mut entities,
            &event(1, index as u64, from, to, U256::from(amount)),
            &contract,
            1001,
        )
        .await
        .unwrap();
    }
    assert_balance(&entities, &contract, Some(a), "0", "0");
    assert_balance(&entities, &contract, Some(b), "100", "0.0000000000000001");
    assert_balance(&entities, &contract, None, "100", "0.0000000000000001");
    assert_eq!(
        entities
            .0
            .keys()
            .filter(|(kind, _)| kind == "Transaction")
            .count(),
        1
    );
    assert_eq!(
        entities
            .0
            .keys()
            .filter(|(kind, _)| kind == "ERC20Transfer")
            .count(),
        4
    );
}

#[tokio::test]
/// Verifies that partial history can contain signed balances.
async fn partial_history_produces_signed_balances_like_the_reference_mapping() {
    let mut entities = MemoryEntities::default();
    let contract = contract(2);
    save_contract(&mut entities, &contract).await.unwrap();
    let a = Address::repeat_byte(1);
    let b = Address::repeat_byte(2);
    apply_transfer(
        &mut entities,
        &event(50, 7, a, b, U256::from(25)),
        &contract,
        1050,
    )
    .await
    .unwrap();
    assert_balance(&entities, &contract, Some(a), "-25", "-0.25");
    assert_balance(&entities, &contract, Some(b), "25", "0.25");
    assert_balance(&entities, &contract, None, "0", "0");
}

#[test]
/// Verifies decimal formatting without floating-point conversion.
fn decimal_values_use_graph_node_precision_without_floating_point() {
    for (exact, decimals, expected) in [
        ("0", 18, "0"),
        ("123456789", 6, "123.456789"),
        ("-25", 2, "-0.25"),
        ("100", 0, "100"),
        (
            "12345678901234567890123456789012345",
            18,
            "12345678901234567.89012345678901235",
        ),
        (
            "99999999999999999999999999999999999",
            0,
            "100000000000000000000000000000000000",
        ),
        ("1", 255, ""),
    ] {
        let value = decimal_value(&BigInt::from_str(exact).unwrap(), decimals);
        if decimals == 255 {
            assert_eq!(value, format!("0.{}1", "0".repeat(254)));
        } else {
            assert_eq!(value, expected);
        }
    }
}

#[tokio::test]
/// Verifies that unavailable metadata falls back once and is then persisted.
async fn reverted_metadata_defaults_are_saved_only_once() {
    let responses = Asserter::new();
    for _ in 0..3 {
        responses.push_failure_msg("execution reverted");
    }
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(responses.clone());
    let handler = TransferHandler::new(Arc::new(provider), Duration::from_secs(1));
    let mut entities = MemoryEntities::default();
    let event = event(1, 0, Address::ZERO, Address::repeat_byte(1), U256::from(1));
    let token = event.address;
    let contract = handler
        .contract(&mut entities, &event, token)
        .await
        .unwrap();
    assert_eq!(contract.name, None);
    assert_eq!(contract.symbol, None);
    assert_eq!(contract.decimals, 18);
    // A second fetch needs no response: persisted metadata wins.
    handler
        .contract(&mut entities, &event, token)
        .await
        .unwrap();
    assert!(responses.read_q().is_empty());
}

#[tokio::test]
/// Verifies invalid ABI output defaults while transport failures abort contract creation.
async fn invalid_abi_uses_defaults_but_rpc_errors_do_not_create_contracts() {
    let responses = Asserter::new();
    for _ in 0..3 {
        responses.push_success(&Bytes::new());
    }
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(responses.clone());
    let handler = TransferHandler::new(Arc::new(provider), Duration::from_secs(1));
    let event = event(1, 0, Address::ZERO, Address::repeat_byte(1), U256::from(1));
    let mut entities = MemoryEntities::default();
    handler
        .contract(&mut entities, &event, event.address)
        .await
        .unwrap();
    assert_eq!(
        entities.entity("ERC20Contract", &address_id(event.address))["decimals"],
        18
    );

    responses.push_failure_msg("historical state unavailable");
    let mut unavailable = MemoryEntities::default();
    assert!(matches!(
        handler
            .contract(&mut unavailable, &event, event.address)
            .await,
        Err(RavenError::Source(_))
    ));
    assert!(unavailable.0.is_empty());
}

#[tokio::test]
/// Verifies metadata and timestamps are reused for transfers in one transaction.
async fn handler_reads_metadata_and_timestamp_then_reuses_them_for_the_same_transaction() {
    let responses = Asserter::new();
    responses.push_success(&Bytes::from(IERC20Metadata::nameCall::abi_encode_returns(
        &"Test Token".to_owned(),
    )));
    responses.push_success(&Bytes::from(
        IERC20Metadata::symbolCall::abi_encode_returns(&"TEST".to_owned()),
    ));
    responses.push_success(&Bytes::from(
        IERC20Metadata::decimalsCall::abi_encode_returns(&6),
    ));
    let first = event(
        1,
        0,
        Address::ZERO,
        Address::repeat_byte(1),
        U256::from(1_000_000),
    );
    let mut block: alloy_rpc_types_eth::Block = alloy_rpc_types_eth::Block::default();
    block.header.hash = first.block_hash;
    block.header.inner.number = first.block_number;
    block.header.inner.timestamp = 1500;
    responses.push_success(&block);
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(responses.clone());
    let handler = TransferHandler::new(Arc::new(provider), Duration::from_secs(1));
    let mut entities = MemoryEntities::default();
    handler.handle(&mut entities, &first).await.unwrap();
    let token = address_id(first.address);
    assert_eq!(
        entities.entity("ERC20Contract", &token)["name"],
        "Test Token"
    );
    assert_eq!(entities.entity("ERC20Contract", &token)["symbol"], "TEST");
    assert_eq!(entities.entity("ERC20Contract", &token)["decimals"], 6);
    assert_eq!(entities.entity("ERC20Transfer", "1-0")["value"], "1");
    assert_eq!(entities.entity("ERC20Transfer", "1-0")["timestamp"], "1500");
    let second = event(
        1,
        1,
        Address::repeat_byte(1),
        Address::repeat_byte(1),
        U256::from(10),
    );
    handler.handle(&mut entities, &second).await.unwrap();
    assert_eq!(entities.entity("ERC20Transfer", "1-1")["timestamp"], "1500");
    assert!(responses.read_q().is_empty());
}

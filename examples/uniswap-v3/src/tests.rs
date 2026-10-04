use crate::{
    ChainConfig,
    entities::{Bundle, Factory, Pool, Token, new_entity},
    handlers::{Context, Mapping},
    math::{decimal, integer, power, round34, units},
};
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_eth::{Block, BlockTransactions, Transaction};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use raven_engine::{Entity, EntityStore, EntityStoreExt, RavenResult};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
struct Memory(BTreeMap<(String, String), Value>);
#[async_trait]
impl EntityStore for Memory {
    /// Returns a cloned entity from the in-memory test store.
    async fn get(&mut self, kind: &str, id: &str) -> RavenResult<Option<Value>> {
        Ok(self.0.get(&(kind.into(), id.into())).cloned())
    }
    /// Stores a cloned entity in the in-memory test store.
    async fn put(&mut self, kind: &str, id: &str, data: &Value) -> RavenResult<()> {
        self.0.insert((kind.into(), id.into()), data.clone());
        Ok(())
    }
    /// Removes an entity from the in-memory test store.
    async fn delete(&mut self, kind: &str, id: &str) -> RavenResult<()> {
        self.0.remove(&(kind.into(), id.into()));
        Ok(())
    }
}
/// Asserts a JSON decimal's canonical string representation.
fn assert_decimal(value: &Value, expected: &str) {
    let actual: BigDecimal = serde_json::from_value(value.clone()).expect("decimal entity field");
    assert_eq!(actual, decimal(expected));
}
/// Creates a mapping backed by the standard local test provider.
fn mapping() -> Mapping {
    Mapping::new(
        Arc::new(
            ProviderBuilder::new()
                .disable_recommended_fillers()
                .connect_http("http://127.0.0.1:1".parse().unwrap()),
        ),
        "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap(),
        "0x0000000000000000000000000000000000000008"
            .parse()
            .unwrap(),
        ChainConfig::load(None).unwrap(),
    )
}
/// Builds a block and transaction suitable for mapping context tests.
fn cached_block(
    hash: alloy_primitives::B256,
    number: u64,
    timestamp: u64,
    from: alloy_primitives::Address,
    gas_price: u128,
) -> (Block, alloy_primitives::B256) {
    let transaction: Transaction = serde_json::from_value(json!({
        "blockHash":hash, "blockNumber":format!("0x{number:x}"), "transactionIndex":"0x0",
        "from":from, "gas":"0x5208", "gasPrice":format!("0x{gas_price:x}"), "hash":alloy_primitives::B256::ZERO,
        "input":"0x", "nonce":"0x0", "to":from, "value":"0x0", "v":"0x1b", "r":"0x1", "s":"0x1", "type":"0x0", "chainId":"0x1"
    })).expect("test transaction");
    let mut block: Block = Block::default();
    block.header.hash = hash;
    block.header.inner.number = number;
    block.header.inner.timestamp = timestamp;
    block.transactions = BlockTransactions::Full(vec![transaction]);
    let tx = *block
        .transactions
        .as_transactions()
        .expect("full transaction")[0]
        .inner
        .tx_hash();
    (block, tx)
}
struct RpcFixture {
    task: tokio::task::JoinHandle<()>,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
}
impl Drop for RpcFixture {
    /// Aborts the fixture server when its owner is dropped.
    fn drop(&mut self) {
        self.task.abort();
    }
}
/// Creates a mapping whose RPC calls are answered by the supplied fixture.
async fn rpc_mapping<F>(respond: F) -> (Mapping, RpcFixture)
where
    F: Fn(&Value) -> Result<String, String> + Send + Sync + 'static,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let respond = Arc::new(respond);
    let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let respond = Arc::clone(&respond);
            let recorded = Arc::clone(&recorded);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let request = loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() < end + 4 + length {
                        continue;
                    }
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                };
                recorded.lock().await.push(request.clone());
                let response=match respond(&request) {
                Ok(value)=>json!({"jsonrpc":"2.0","id":request["id"],"result":value}),
                Err(message)=>json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":3,"message":message}}),
            }.to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            });
        }
    });
    let mut value = mapping();
    value.provider = Arc::new(
        ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(format!("http://{address}").parse().unwrap()),
    );
    (value, RpcFixture { task, requests })
}
#[test]
/// Verifies decimal price and signed-unit calculations preserve required precision.
fn decimal_prices_and_signed_units_preserve_graph_precision() {
    assert_eq!(units("-1234567", "6"), decimal("-1.234567"));
    assert_eq!(
        round34(integer("1") / integer("3")),
        decimal("0.3333333333333333333333333333333333")
    );
    assert_eq!(
        power(&decimal("1.0001"), -1),
        decimal("0.99990000999900009999000099990001")
    );
    assert_eq!(
        units("123456789012345678901234567890123456789", "0"),
        decimal("123456789012345678901234567890123500000")
    );
}

#[test]
/// Verifies integer entities remain exact while decimal operations use 34 digits.
fn entity_uint256_round_trip_is_exact_while_decimal_operations_round_to_34_digits() {
    let maximum =
        integer("115792089237316195423570985008687907853269984665640564039457584007913129639935");
    let token = Token {
        id: "token".into(),
        total_supply: maximum.clone(),
        ..Default::default()
    };
    let json = serde_json::to_value(&token).unwrap();
    assert_eq!(json["totalSupply"], maximum.to_string());
    let restored: Token = serde_json::from_value(json).unwrap();
    assert_eq!(restored.total_supply, maximum);

    let exact = integer("123456789012345678901234567890123456789");
    assert_ne!(exact, round34(exact.clone()));
    assert_eq!(
        decimal("123456789012345678901234567890123456789"),
        round34(exact)
    );
}
#[tokio::test]
/// Verifies interval rows use committed state across hour and day boundaries.
async fn intervals_use_committed_mapping_state_and_cross_hour_day_boundaries() {
    let mut store = Memory::default();
    let factory = Factory {
        id: "factory".into(),
        tx_count: integer("9"),
        total_value_locked_usd: decimal("300"),
        ..Default::default()
    };
    store.save(&factory).await.unwrap();
    let pool = Pool {
        id: "pool".into(),
        liquidity: integer("50"),
        token0_price: decimal("2"),
        total_value_locked_usd: decimal("100"),
        ..Default::default()
    };
    store.save(&pool).await.unwrap();
    let bundle = Bundle {
        id: "1".into(),
        eth_price_usd: decimal("10"),
    };
    store.save(&bundle).await.unwrap();
    let token0 = Token {
        id: "t0".into(),
        derived_eth: decimal("3"),
        ..Default::default()
    };
    let token1 = Token {
        id: "t1".into(),
        ..Default::default()
    };
    let mut context = Context {
        timestamp: 86399,
        number: 10,
        tx: "tx".into(),
        log_index: 1,
        origin: "owner".into(),
        gas_price: integer("1"),
    };
    crate::intervals::update(
        &mut store, &context, "factory", "pool", &token0, &token1, None,
    )
    .await
    .unwrap();
    let first = store.get("PoolDayData", "pool-0").await.unwrap().unwrap();
    assert_eq!(first["liquidity"], "50");
    assert_decimal(&first["tvlUSD"], "100");
    assert_eq!(first["txCount"], "1");
    assert_eq!(
        store.get("UniswapDayData", "0").await.unwrap().unwrap()["txCount"],
        "9"
    );
    context.timestamp = 86401;
    crate::intervals::update(
        &mut store, &context, "factory", "pool", &token0, &token1, None,
    )
    .await
    .unwrap();
    assert_eq!(
        store.get("PoolDayData", "pool-1").await.unwrap().unwrap()["date"],
        json!(86400)
    );
    assert!(
        store
            .get("PoolHourData", "pool-23")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .get("PoolHourData", "pool-24")
            .await
            .unwrap()
            .is_some()
    );
    assert_decimal(
        &store.get("TokenDayData", "t0-1").await.unwrap().unwrap()["priceUSD"],
        "30",
    );
    assert_eq!(
        store.get("PoolDayData", "pool-0").await.unwrap().unwrap(),
        first
    );
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique schema"]
/// Verifies native entities round-trip and restore after reopening storage.
async fn all_official_native_entities_round_trip_and_restore_after_restart() {
    use raven_engine::{BlockPtr, ChainStore, EntityChange, LiteBlockHeader, Network};
    use raven_postgres::PostgresChainStore;
    let url = std::env::var("RAVEN_TEST_DATABASE_URL").expect("dedicated test database");
    let schema = format!(
        "uniswap_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
    );
    type Store = PostgresChainStore<String, u64>;
    let admin = sqlx::PgPool::connect(&url).await.expect("test connection");
    let store = Store::connect_with_schema_sql(
        &url,
        &schema,
        "test",
        crate::storage::SCHEMA_SQL,
        crate::storage::Storage,
    )
    .await
    .unwrap();
    store
        .initialize(&Network::new(1, 0, None).unwrap())
        .await
        .unwrap();
    let kinds = [
        "Factory",
        "Bundle",
        "Token",
        "Pool",
        "Tick",
        "Transaction",
        "Mint",
        "Burn",
        "Swap",
        "Collect",
        "Flash",
        "UniswapDayData",
        "PoolDayData",
        "PoolHourData",
        "TokenDayData",
        "TokenHourData",
        "Position",
        "PositionSnapshot",
        "TickDayData",
        "TickHourData",
    ];
    let mut originals = Vec::new();
    for kind in kinds {
        let default_id = format!("{kind}-defaults");
        originals.push(EntityChange {
            entity_type: kind.into(),
            entity_id: default_id.clone(),
            data: Some(new_entity(kind, &default_id)),
        });
        let mut value = new_entity(kind, kind);
        // Distinct field values detect crossed SQL columns; defaults also cover nullable fields.
        for (index, (field, data)) in value.as_object_mut().unwrap().iter_mut().enumerate() {
            if field == "id" || (kind == "Transaction" && field == "blockNumber") {
                continue;
            }
            let number = index + 101;
            *data = match data {
                Value::String(text) if text == "0" => json!(number.to_string()),
                Value::String(_) => json!(format!("{kind}-{field}")),
                Value::Number(_) => json!(number),
                Value::Array(_) => json!(["p1", "p2"]),
                Value::Null if matches!(field.as_str(), "owner" | "sender") => {
                    json!(format!("{kind}-{field}"))
                }
                Value::Null if field == "amountUSD" => json!(format!("{number}.25")),
                Value::Null => json!(number.to_string()),
                _ => panic!("unexpected entity field"),
            };
        }
        originals.push(EntityChange {
            entity_type: kind.into(),
            entity_id: kind.into(),
            data: Some(value),
        });
    }
    let b0 = LiteBlockHeader {
        number: 0,
        hash: "b0".into(),
        parent_hash: "parent".into(),
    };
    let b1 = LiteBlockHeader {
        number: 1,
        hash: "b1".into(),
        parent_hash: "b0".into(),
    };
    store
        .commit_block(None, &b0, originals.clone())
        .await
        .unwrap();
    for entity in &originals {
        assert_eq!(
            store
                .get_entity(
                    Some(&BlockPtr::from(&b0)),
                    &entity.entity_type,
                    &entity.entity_id
                )
                .await
                .unwrap(),
            entity.data
        );
    }
    let before: sqlx::types::Json<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY entity_type,entity_id) FROM {schema}.pool_state t"
    )))
    .fetch_one(&admin)
    .await
    .unwrap();
    let mut token = new_entity("Token", "Token");
    token["whitelistPools"] = json!(["changed"]);
    token["totalValueLocked"] = json!("42.25");
    store
        .commit_block(
            Some(&BlockPtr::from(&b0)),
            &b1,
            vec![EntityChange {
                entity_type: "Token".into(),
                entity_id: "Token".into(),
                data: Some(token),
            }],
        )
        .await
        .unwrap();
    store.close().await;
    let store = Store::connect_with_schema_sql(
        &url,
        &schema,
        "test",
        crate::storage::SCHEMA_SQL,
        crate::storage::Storage,
    )
    .await
    .unwrap();
    store.revert_block(&b1).await.unwrap();
    let after: sqlx::types::Json<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY entity_type,entity_id) FROM {schema}.pool_state t"
    )))
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(before, after);
    let mut invalid = new_entity("Swap", "bad");
    invalid["amount0"] = json!("invalid-number");
    assert!(
        store
            .commit_block(
                Some(&BlockPtr::from(&b0)),
                &b1,
                vec![EntityChange {
                    entity_type: "Swap".into(),
                    entity_id: "bad".into(),
                    data: Some(invalid)
                }]
            )
            .await
            .is_err()
    );
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b0)));
    store.revert_block(&b0).await.unwrap();
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {schema}.pool_state"
    )))
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(count, 0);
    store.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
/// Verifies volume and liquidity pricing follow the configured token policy.
async fn tracked_volume_and_liquidity_pricing_follow_reference_policy() {
    let mut mapping = mapping();
    let reference = mapping.chain.reference_token.clone();
    let token0 = Token {
        id: reference.clone(),
        derived_eth: integer("1"),
        ..Default::default()
    };
    let mut token1 = Token {
        id: "target".into(),
        derived_eth: decimal("3"),
        ..Default::default()
    };
    let bundle = Bundle {
        id: "1".into(),
        eth_price_usd: decimal("10"),
    };
    mapping.chain.whitelist_tokens = vec![reference.clone(), "target".into()];
    assert_eq!(
        mapping.tracked(decimal("2"), &token0, decimal("4"), &token1, &bundle),
        decimal("140")
    );
    mapping.chain.whitelist_tokens = vec![reference.clone()];
    assert_eq!(
        mapping.tracked(decimal("2"), &token0, decimal("4"), &token1, &bundle),
        decimal("40")
    );
    mapping.chain.minimum_native_locked = "20".into();
    mapping.chain.whitelist_tokens.clear();
    assert_eq!(
        mapping.tracked(decimal("2"), &token0, decimal("4"), &token1, &bundle),
        BigDecimal::from(0)
    );
    let mut store = Memory::default();
    store.save(&bundle).await.unwrap();
    store.save(&token0).await.unwrap();
    token1.whitelist_pools = vec!["small".into(), "large".into()];
    for (id, locked, price) in [("small", "10", "0.5"), ("large", "50", "0.25")] {
        let pool = Pool {
            id: id.into(),
            token0: "target".into(),
            token1: reference.clone(),
            liquidity: integer("100"),
            total_value_locked_token1: decimal(locked),
            token1_price: decimal(price),
            ..Default::default()
        };
        store.save(&pool).await.unwrap();
    }
    assert_eq!(
        mapping.derived(&mut store, &token1).await.unwrap(),
        decimal("0.25")
    );
    mapping.chain.stable_coins = vec!["stable".into()];
    let zero_bundle = Bundle {
        id: "1".into(),
        eth_price_usd: BigDecimal::from(0),
    };
    store.save(&zero_bundle).await.unwrap();
    let stable = Token {
        id: "stable".into(),
        ..Default::default()
    };
    assert_eq!(
        mapping.derived(&mut store, &stable).await.unwrap(),
        BigDecimal::from(0)
    );
}

#[tokio::test]
/// Verifies logs from untracked pools are ignored before event decoding.
async fn malformed_untracked_pool_logs_are_ignored_before_decoding() {
    use alloy_primitives::{Address, B256, Bytes, Log};
    use alloy_sol_types::SolEvent;
    use raven_engine::Handler;
    let mut store = Memory::default();
    let address = Address::repeat_byte(2);
    let log = raven_evm::LogUpdate {
        log: Log::new_unchecked(address, vec![crate::Swap::SIGNATURE_HASH], Bytes::new()),
        block_number: 1,
        block_hash: B256::ZERO,
        transaction_hash: B256::ZERO,
        transaction_index: 0,
        log_index: 0,
    };
    let handler = super::MappingHandler(Arc::new(mapping()));
    handler.handle(&mut store, &log).await.unwrap();
    assert!(store.0.is_empty());
    let id = address.to_string().to_lowercase();
    let pool = Pool {
        id: id.clone(),
        ..Default::default()
    };
    store.save(&pool).await.unwrap();
    assert!(handler.handle(&mut store, &log).await.is_err());
}

#[tokio::test]
/// Verifies Mint, Burn, and Swap preserve the intended USD rounding order.
async fn mint_burn_and_swap_keep_the_reference_usd_rounding_order() {
    use alloy_primitives::{Address, B256, Bytes, Log};
    use raven_evm::LogUpdate;
    let (mut mapping, _rpc) = rpc_mapping(|_| Ok(format!("0x{}", "00".repeat(32 * 8)))).await;
    let reference = mapping.chain.reference_token.clone();
    mapping.chain.stable_token_pool = "stable-price".into();
    let address = Address::repeat_byte(2);
    let pool_id = address.to_string().to_lowercase();
    let hash = B256::repeat_byte(3);
    let (block, tx) = cached_block(hash, 1, 0x15180, address, 1);
    *mapping.block.lock().await = Some((hash, block));
    let mut store = Memory::default();
    let pool = Pool {
        id: pool_id.clone(),
        token0: "target".into(),
        token1: reference.clone(),
        tick: Some(integer("0")),
        fee_tier: integer("3000"),
        ..Default::default()
    };
    let token0 = Token {
        id: "target".into(),
        decimals: integer("34"),
        derived_eth: decimal("0.3333333333333333333333333333333333"),
        whitelist_pools: vec!["pricing".into()],
        ..Default::default()
    };
    let token1 = Token {
        id: reference.clone(),
        decimals: integer("34"),
        derived_eth: integer("1"),
        ..Default::default()
    };
    let bundle = Bundle {
        id: "1".into(),
        eth_price_usd: decimal("3.141592653589793238462643383279503"),
    };
    let pricing = Pool {
        id: "pricing".into(),
        token0: "target".into(),
        token1: reference.clone(),
        liquidity: integer("1"),
        total_value_locked_token1: decimal("100"),
        token1_price: decimal("0.3333333333333333333333333333333333"),
        ..Default::default()
    };
    let stable = Pool {
        id: "stable-price".into(),
        token0: reference.clone(),
        token1_price: decimal("3.141592653589793238462643383279503"),
        ..Default::default()
    };
    let factory = Factory {
        id: mapping.factory.clone(),
        ..Default::default()
    };
    store.save(&factory).await.unwrap();
    for pool in [&pool, &pricing, &stable] {
        store.save(pool).await.unwrap();
    }
    for token in [&token0, &token1] {
        store.save(token).await.unwrap();
    }
    store.save(&bundle).await.unwrap();
    let mut event = LogUpdate {
        log: Log::new_unchecked(address, vec![], Bytes::new()),
        block_number: 1,
        block_hash: hash,
        transaction_hash: tx,
        transaction_index: 0,
        log_index: 1,
    };
    mapping
        .liquidity_event(
            &mut store,
            &event.parsed(()),
            super::LiquidityKind::Mint,
            address,
            Some(address),
            -1,
            1,
            "1".into(),
            "3333333333333333333333333333333333".into(),
            "0".into(),
        )
        .await
        .unwrap();
    // Independent decimal oracle, precision=34, ROUND_HALF_UP. Grouping differs by one final digit.
    assert_decimal(
        &store.get("Token", "target").await.unwrap().unwrap()["totalValueLockedUSD"],
        "0.3490658503988659153847381536977226",
    );
    event.log_index = 2;
    mapping
        .liquidity_event(
            &mut store,
            &event.parsed(()),
            super::LiquidityKind::Burn,
            address,
            None,
            -1,
            1,
            "0".into(),
            "100000000000000000000000000000000".into(),
            "0".into(),
        )
        .await
        .unwrap();
    assert_decimal(
        &store.get("Token", "target").await.unwrap().unwrap()["totalValueLockedUSD"],
        "0.338593874886899937923196009086791",
    );
    event.log_index = 3;
    let swap = event.parsed(crate::Swap {
        sender: address,
        recipient: address,
        amount0: "0".parse().unwrap(),
        amount1: "0".parse().unwrap(),
        sqrtPriceX96: "79228162514264337593543950336".parse().unwrap(),
        liquidity: 1,
        tick: "0".parse().unwrap(),
    });
    mapping.swapped(&mut store, &swap).await.unwrap();
    assert_decimal(
        &store.get("Token", "target").await.unwrap().unwrap()["totalValueLockedUSD"],
        "0.3385938748868999379231960090867909",
    );
}

#[tokio::test]
/// Verifies NFT events update positions and share a single block snapshot.
async fn nft_events_preserve_deployed_collect_behavior_and_same_block_snapshot() {
    use crate::events::PositionManager;
    use alloy_primitives::{Address, B256, U256};
    use alloy_sol_types::{SolCall, SolEvent};
    use raven_engine::Handler;
    let token0 = Address::repeat_byte(11);
    let token1 = Address::repeat_byte(12);
    let pool = Address::repeat_byte(2);
    let (mapping, rpc) = rpc_mapping(move |request| {
        let input = request["params"][0]["input"]
            .as_str()
            .or_else(|| request["params"][0]["data"].as_str())
            .unwrap();
        let selector = format!(
            "0x{}",
            alloy_primitives::hex::encode(PositionManager::positionsCall::SELECTOR)
        );
        if input.starts_with(&selector) {
            let word = |number: u64| format!("{number:064x}");
            let address = |value: Address| {
                format!(
                    "{}{}",
                    "00".repeat(12),
                    alloy_primitives::hex::encode(value)
                )
            };
            return Ok(format!(
                "0x{}",
                [
                    word(0),
                    address(Address::ZERO),
                    address(token0),
                    address(token1),
                    word(3000),
                    format!("{}c4", "ff".repeat(31)),
                    word(60),
                    word(999),
                    word(33),
                    word(44),
                    word(0),
                    word(0)
                ]
                .concat()
            ));
        }
        Ok(format!(
            "0x{}{}",
            "00".repeat(12),
            alloy_primitives::hex::encode(pool)
        ))
    })
    .await;
    let hash = B256::repeat_byte(3);
    let (block, tx) = cached_block(hash, 10, 0x15180, token0, 7);
    *mapping.block.lock().await = Some((hash, block));
    let manager = mapping.position_manager;
    let handler = super::MappingHandler(Arc::new(mapping));
    let mut store = Memory::default();
    for (address, decimals) in [(token0, 2), (token1, 3)] {
        let id = address.to_string().to_lowercase();
        let token = Token {
            id: id.clone(),
            decimals: integer(&decimals.to_string()),
            ..Default::default()
        };
        store.save(&token).await.unwrap();
    }
    let id = U256::from(7);
    let owner = Address::repeat_byte(5);
    let events = vec![
        PositionManager::IncreaseLiquidity {
            tokenId: id,
            liquidity: 10,
            amount0: U256::from(1234),
            amount1: U256::from(56789),
        }
        .encode_log_data(),
        PositionManager::DecreaseLiquidity {
            tokenId: id,
            liquidity: 3,
            amount0: U256::from(234),
            amount1: U256::from(789),
        }
        .encode_log_data(),
        PositionManager::Collect {
            tokenId: id,
            recipient: owner,
            amount0: U256::from(50),
            amount1: U256::from(900),
        }
        .encode_log_data(),
        PositionManager::Transfer {
            from: Address::ZERO,
            to: owner,
            tokenId: id,
        }
        .encode_log_data(),
    ];
    for (index, data) in events.into_iter().enumerate() {
        let log = raven_evm::LogUpdate {
            log: alloy_primitives::Log {
                address: manager,
                data,
            },
            block_number: 10,
            block_hash: hash,
            transaction_hash: tx,
            transaction_index: 0,
            log_index: index as u64,
        };
        handler.handle(&mut store, &log).await.unwrap();
    }
    let position = store.get("Position", "7").await.unwrap().unwrap();
    assert_eq!(position["liquidity"], "7");
    assert_decimal(&position["depositedToken0"], "12.34");
    assert_decimal(&position["depositedToken1"], "56.789");
    assert_decimal(&position["withdrawnToken0"], "2.34");
    assert_decimal(&position["withdrawnToken1"], "0.789");
    assert_decimal(&position["collectedFeesToken0"], "0.5");
    assert_decimal(&position["collectedFeesToken1"], "0.5");
    assert_eq!(
        position["tickLower"],
        format!("{}#-60", pool.to_string().to_lowercase())
    );
    assert_eq!(position["feeGrowthInside0LastX128"], "33");
    assert_eq!(position["feeGrowthInside1LastX128"], "44");
    let snapshot = store
        .get("PositionSnapshot", "7#10")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["owner"], owner.to_string().to_lowercase());
    assert_eq!(snapshot["liquidity"], "7");
    assert_eq!(snapshot["blockNumber"], "10");
    assert_eq!(
        store
            .0
            .keys()
            .filter(|(k, _)| k == "PositionSnapshot")
            .count(),
        1
    );
    assert!(!store.0.keys().any(|(k, _)| k == "Collect"));
    for request in rpc.requests.lock().await.iter() {
        assert_eq!(
            request["params"][1]["blockHash"],
            hash.to_string(),
            "{}",
            request["params"][1]
        );
    }
}

#[tokio::test]
/// Verifies missing positions and foreign transfers create no indexed state.
async fn deleted_nft_positions_and_foreign_transfers_are_ignored() {
    use alloy_primitives::{Address, B256, U256};
    use alloy_sol_types::SolEvent;
    use raven_engine::Handler;
    use raven_evm::Parser;
    let (mapping, _rpc) = rpc_mapping(|_| Err("execution reverted: position missing".into())).await;
    let hash = B256::repeat_byte(3);
    let manager = mapping.position_manager;
    let (block, tx) = cached_block(hash, 1, 1, manager, 1);
    *mapping.block.lock().await = Some((hash, block));
    let event = crate::PositionManager::Transfer {
        from: Address::ZERO,
        to: manager,
        tokenId: U256::from(8),
    };
    let mut log = raven_evm::LogUpdate {
        log: alloy_primitives::Log {
            address: manager,
            data: event.encode_log_data(),
        },
        block_number: 1,
        block_hash: hash,
        transaction_hash: tx,
        transaction_index: 0,
        log_index: 1,
    };
    let parser = crate::parser::UniswapParser::new(mapping.factory.parse().unwrap(), manager);
    assert!(
        parser
            .parse(&raven_evm::Update::Log(log.clone()))
            .await
            .unwrap()
            .is_some()
    );
    let handler = super::MappingHandler(Arc::new(mapping));
    let mut store = Memory::default();
    handler.handle(&mut store, &log).await.unwrap();
    assert!(store.0.is_empty());
    log.log.address = Address::repeat_byte(9);
    assert!(
        parser
            .parse(&raven_evm::Update::Log(log.clone()))
            .await
            .unwrap()
            .is_none()
    );
    handler.handle(&mut store, &log).await.unwrap();
    assert!(store.0.is_empty());
}

#[tokio::test]
/// Verifies Flash fee growth and tick daily fields retain their copy order.
async fn flash_updates_fee_growth_and_tick_daily_fields_keep_reference_copy_order() {
    use alloy_primitives::{Address, B256, U256};
    use raven_evm::LogUpdate;
    let (mapping, _rpc) = rpc_mapping(|_| Ok(format!("0x{:064x}", 77))).await;
    let address = Address::repeat_byte(2);
    let id = address.to_string().to_lowercase();
    let hash = B256::repeat_byte(3);
    let mut store = Memory::default();
    let pool = Pool {
        id: id.clone(),
        ..Default::default()
    };
    store.save(&pool).await.unwrap();
    let log = LogUpdate {
        log: alloy_primitives::Log::new_unchecked(address, vec![], Default::default()),
        block_number: 10,
        block_hash: hash,
        transaction_hash: B256::ZERO,
        transaction_index: 0,
        log_index: 0,
    };
    mapping
        .flashed(
            &mut store,
            &log.parsed(crate::events::Flash {
                sender: address,
                recipient: address,
                amount0: U256::from(1),
                amount1: U256::from(2),
                paid0: U256::from(3),
                paid1: U256::from(4),
            }),
        )
        .await
        .unwrap();
    let pool = store.get(Pool::ENTITY_NAME, &id).await.unwrap().unwrap();
    assert_eq!(pool["feeGrowthGlobal0X128"], "77");
    assert_eq!(pool["feeGrowthGlobal1X128"], "77");
    assert_eq!(pool["txCount"], "0");
    assert_eq!(store.0.len(), 1);
    let context = Context {
        timestamp: 86401,
        number: 10,
        tx: "tx".into(),
        log_index: 0,
        origin: id.clone(),
        gas_price: integer("1"),
    };
    let mut tick = super::new_tick(&id, 60, &context);
    tick.volume_token0 = decimal("11");
    tick.volume_token1 = decimal("22");
    tick.fee_growth_outside0_x128 = integer("33");
    crate::intervals::tick_day(&mut store, &context, &tick)
        .await
        .unwrap();
    let day = store
        .get("TickDayData", &format!("{id}#60-1"))
        .await
        .unwrap()
        .unwrap();
    assert_decimal(&day["volumeToken0"], "11");
    assert_decimal(&day["volumeToken1"], "11");
    assert_eq!(day["feeGrowthOutside0X128"], "33");
    assert!(!store.0.keys().any(|(k, _)| k == "TickHourData"));
}

#[tokio::test]
/// Verifies crossed ticks use signed spacing and read only initialized state.
async fn crossed_ticks_preserve_signed_remainder_and_update_current_tick_before_limit() {
    use crate::events::PoolMetadata;
    use alloy_primitives::{Address, B256};
    use alloy_sol_types::SolCall;
    let (mapping, rpc) = rpc_mapping(|_| {
        Ok(format!(
            "0x{}",
            [0u64, 0, 33, 44, 0, 0, 0, 1]
                .map(|n| format!("{n:064x}"))
                .concat()
        ))
    })
    .await;
    let address = Address::repeat_byte(2);
    let id = address.to_string().to_lowercase();
    let hash = B256::repeat_byte(3);
    let event = raven_evm::LogUpdate {
        log: alloy_primitives::Log::new_unchecked(address, vec![], Default::default()),
        block_number: 10,
        block_hash: hash,
        transaction_hash: B256::ZERO,
        transaction_index: 0,
        log_index: 0,
    }
    .parsed(());
    let context = Context {
        timestamp: 86401,
        number: 10,
        tx: "tx".into(),
        log_index: 0,
        origin: id.clone(),
        gas_price: integer("1"),
    };
    let mut store = Memory::default();
    for index in [2, 0, -10, -9, -1010] {
        let tick = super::new_tick(&id, index, &context);
        store.save(&tick).await.unwrap();
    }
    let mut pool = Pool {
        id: id.clone(),
        fee_tier: integer("500"),
        tick: Some(integer("-9")),
        ..Default::default()
    };
    // The descending loop starts at previous - (new % spacing), i.e. 2.
    mapping
        .crossed_ticks(&mut store, &pool, Some((-7).into()), &event, &context)
        .await
        .unwrap();
    assert_eq!(
        store
            .get("Tick", &format!("{id}#2"))
            .await
            .unwrap()
            .unwrap()["feeGrowthOutside0X128"],
        "33"
    );
    assert_eq!(
        store
            .get("Tick", &format!("{id}#0"))
            .await
            .unwrap()
            .unwrap()["feeGrowthOutside0X128"],
        "0"
    );
    assert_eq!(
        store
            .get("Tick", &format!("{id}#-10"))
            .await
            .unwrap()
            .unwrap()["feeGrowthOutside0X128"],
        "0"
    );
    pool.tick = Some(integer("-1010"));
    mapping
        .crossed_ticks(&mut store, &pool, Some(10.into()), &event, &context)
        .await
        .unwrap();
    assert_eq!(
        store
            .get("Tick", &format!("{id}#-1010"))
            .await
            .unwrap()
            .unwrap()["feeGrowthOutside1X128"],
        "44"
    );
    assert!(
        store
            .get("TickDayData", &format!("{id}#-1010-1"))
            .await
            .unwrap()
            .is_some()
    );
    let requests = rpc.requests.lock().await;
    assert_eq!(
        requests.len(),
        2,
        "long crossings must only read the current initialized tick"
    );
    for (request, index) in requests.iter().zip([2, -1010]) {
        let input = request["params"][0]["input"].as_str().unwrap();
        let call =
            PoolMetadata::ticksCall::abi_decode(&alloy_primitives::hex::decode(input).unwrap())
                .unwrap();
        assert_eq!(call.tick.to_string(), index.to_string());
        assert_eq!(request["params"][1]["blockHash"], hash.to_string());
    }
}

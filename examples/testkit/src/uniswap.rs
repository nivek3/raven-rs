use std::{collections::BTreeMap, fs};

use bigdecimal::{BigDecimal, num_bigint::BigInt};
use num_traits::Pow;
use serde_json::{Value, json};

use crate::{
    Result,
    kit::Testkit,
    require,
    values::{decimal34_price, hex_bigint, hex_u64, log_words, normalize_json_numbers, text},
};

impl Testkit {
    /// Deploys the fixture contracts and creates an initialized V3 pool.
    pub(crate) fn setup_pool(&mut self) -> Result<(String, u64)> {
        let (weth, start) = self.deploy(
            "uniswap-v3",
            "RavenTestWETH9",
            &[],
            Some("RavenTestTokens.sol"),
        )?;
        let (token_a, _) = self.deploy(
            "uniswap-v3",
            "RavenTestToken",
            &["Raven Token A".into(), "RVA".into(), "18".into()],
            Some("RavenTestTokens.sol"),
        )?;
        let (token_b, _) = self.deploy(
            "uniswap-v3",
            "RavenTestToken",
            &["Raven Token B".into(), "RVB".into(), "18".into()],
            Some("RavenTestTokens.sol"),
        )?;
        let (factory, _) = self.deploy_artifact("core/UniswapV3Factory.json", None, &[])?;
        let (descriptor_library, _) =
            self.deploy_artifact("periphery/NFTDescriptor.json", None, &[])?;
        let descriptor_artifact =
            self.official_artifact("periphery/NonfungibleTokenPositionDescriptor.json")?;
        let references = &descriptor_artifact["linkReferences"]["contracts/libraries/NFTDescriptor.sol"]
            ["NFTDescriptor"];
        let reference = references.as_array().and_then(|refs| refs.first());
        require(reference.is_some(), "missing NFTDescriptor link reference")?;
        let reference = reference.expect("checked above");
        let start_index = reference["start"]
            .as_u64()
            .ok_or("NFTDescriptor link start is not a JSON number")?
            as usize;
        let length = reference["length"]
            .as_u64()
            .ok_or("NFTDescriptor link length is not a JSON number")? as usize;
        let bytecode = text(&descriptor_artifact["bytecode"])?;
        let from = start_index * 2 + 2;
        let to = from + length * 2;
        let link_key = bytecode.get(from..to);
        require(link_key.is_some(), "invalid NFTDescriptor link reference")?;
        let link_key = link_key.expect("checked above").to_owned();
        let (descriptor, _) = self.deploy_artifact(
            "periphery/NonfungibleTokenPositionDescriptor.json",
            Some((
                "f(address,bytes32)",
                &[
                    weth.clone(),
                    "0x4554480000000000000000000000000000000000000000000000000000000000".into(),
                ],
            )),
            &[(link_key, descriptor_library)],
        )?;
        let (position_manager, _) = self.deploy_artifact(
            "periphery/NonfungiblePositionManager.json",
            Some((
                "f(address,address,address)",
                &[factory.clone(), weth.clone(), descriptor],
            )),
            &[],
        )?;
        let (swap_router, _) = self.deploy_artifact(
            "periphery/SwapRouter.json",
            Some(("f(address,address)", &[factory.clone(), weth])),
            &[],
        )?;
        for token in [&token_a, &token_b] {
            for account in self.accounts.iter().take(2).cloned().collect::<Vec<_>>() {
                self.send(
                    token,
                    "mint(address,uint256)",
                    &[account, "1000000000000000000000000".into()],
                    0,
                )?;
                self.send(token, "approve(address,uint256)", &[position_manager.clone(), "115792089237316195423570985008687907853269984665640564039457584007913129639935".into()], 0)?;
            }
            self.send(token, "approve(address,uint256)", &[swap_router.clone(), "115792089237316195423570985008687907853269984665640564039457584007913129639935".into()], 1)?;
        }
        let mut tokens = [token_a, token_b];
        tokens.sort();
        let token0 = tokens[0].clone();
        let token1 = tokens[1].clone();
        self.send(
            &position_manager,
            "createAndInitializePoolIfNecessary(address,address,uint24,uint160)",
            &[
                token0.clone(),
                token1.clone(),
                "3000".into(),
                "79228162514264337593543950336".into(),
            ],
            0,
        )?;
        let pool = text(&self.call(
            &factory,
            "getPool(address,address,uint24)(address)",
            &[token0.clone(), token1.clone(), "3000".into()],
        )?)?
        .to_lowercase();
        require(
            pool != "0x0000000000000000000000000000000000000000",
            "official V3 pool was not created",
        )?;
        self.pool_factory = factory;
        self.pool_address = pool.clone();
        self.position_manager = position_manager;
        self.swap_router = swap_router;
        self.pool_tokens = [token0.clone(), token1.clone()];
        self.pool_config = self.work.join("chain-policy.json");
        fs::write(
            &self.pool_config,
            serde_json::to_vec(&json!({
                "reference_token": token0, "stable_token_pool": pool,
                "minimum_native_locked": "0", "whitelist_tokens": self.pool_tokens,
                "stable_coins": [token1],
            }))?,
        )?;
        Ok((self.pool_address.clone(), start))
    }

    /// Executes the fixture pool lifecycle and returns the minted position token ID.
    pub(crate) fn pool_lifecycle(&mut self) -> Result<String> {
        let [token0, token1] = self.pool_tokens.clone();
        let latest = self.rpc("eth_getBlockByNumber", json!(["latest", false]))?;
        let deadline = hex_u64(&text(&latest["timestamp"])?)? + 3600;
        let owner = self.accounts[0].clone();
        let recipient = self.accounts[2].clone();
        let position_manager = self.position_manager.clone();
        let mint = self.send(&position_manager,
            "mint((address,address,uint24,int24,int24,uint256,uint256,uint256,uint256,address,uint256))",
            &[format!("({token0},{token1},3000,-120,120,1000000000000000000000,1000000000000000000000,0,0,{owner},{deadline})")], 0)?;
        let increase_topic =
            self.event_topic("IncreaseLiquidity(uint256,uint128,uint256,uint256)")?;
        let mint_logs = mint["logs"].as_array().ok_or("mint receipt has no logs")?;
        let increases: Vec<_> = mint_logs
            .iter()
            .filter(|log| {
                log["address"]
                    .as_str()
                    .is_some_and(|a| a.eq_ignore_ascii_case(&position_manager))
                    && log["topics"][0]
                        .as_str()
                        .is_some_and(|topic| topic.eq_ignore_ascii_case(&increase_topic))
            })
            .collect();
        require(
            increases.len() == 1,
            "official NPM mint did not emit one IncreaseLiquidity event",
        )?;
        let token_id = hex_bigint(&text(&increases[0]["topics"][1])?)?.to_string();
        let router = self.swap_router.clone();
        self.send(
            &router,
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
            &[format!(
                "({token0},{token1},3000,{}, {deadline},1000000000000000000,0,0)",
                self.accounts[1]
            )
            .replace(", ", ",")],
            1,
        )?;
        self.send(
            &position_manager,
            "decreaseLiquidity((uint256,uint128,uint256,uint256,uint256))",
            &[format!("({token_id},1,0,0,{deadline})")],
            0,
        )?;
        self.send(&position_manager, "collect((uint256,address,uint128,uint128))",
            &[format!("({token_id},{owner},340282366920938463463374607431768211455,340282366920938463463374607431768211455)")], 0)?;
        self.send(
            &position_manager,
            "safeTransferFrom(address,address,uint256)",
            &[owner, recipient, token_id.clone()],
            0,
        )?;
        Ok(token_id)
    }

    /// Runs smoke checks against pinned official Uniswap V3 contracts.
    pub(crate) fn official_v3_smoke(&mut self) -> Result<()> {
        self.start_anvil()?;
        let (pool, start) = self.setup_pool()?;
        self.check("pinned official V3 Factory and periphery artifacts deploy on owned Anvil");
        let first_position = self.pool_lifecycle()?;
        self.check("official V3 Pool creation, mint, router swap, decrease and collect succeed");
        let owner = text(&self.call(
            &self.position_manager.clone(),
            "ownerOf(uint256)(address)",
            std::slice::from_ref(&first_position),
        )?)?
        .to_lowercase();
        require(
            owner == self.accounts[2].to_lowercase(),
            "official position-manager NFT transfer did not persist",
        )?;
        require(
            text(&self.call(&pool, "liquidity()(uint128)", &[])?)?.parse::<BigInt>()?
                > BigInt::from(0),
            "official V3 pool has no liquidity after mint/decrease",
        )?;
        self.check(
            "official NPM ownership transfer and pool liquidity persist after the lifecycle",
        );
        let name = self.call(&self.pool_tokens[0].clone(), "name()(string)", &[])?;
        let decimals = text(&self.call(&self.pool_tokens[0].clone(), "decimals()(uint8)", &[])?)?
            .parse::<u64>()?;
        let slot0 = self.call(
            &pool,
            "slot0()(uint160,int24,uint16,uint16,uint16,uint8,bool)",
            &[],
        )?;
        let position = self.call(&self.position_manager.clone(), "positions(uint256)(uint96,address,address,address,uint24,int24,int24,uint128,uint256,uint256,uint128,uint128)", std::slice::from_ref(&first_position))?;
        require(
            name.is_string()
                && decimals == 18
                && slot0.as_array().is_some_and(|v| v.len() == 7)
                && position.as_array().is_some_and(|v| v.len() == 12),
            "cast ABI call normalization did not preserve scalar and tuple official contract values",
        )?;
        let position = position.as_array();
        require(
            position.is_some(),
            "NPM positions did not decode as a tuple",
        )?;
        let position = position.expect("checked above");
        require(
            (
                text(&position[2])?.to_lowercase(),
                text(&position[3])?.to_lowercase(),
                text(&position[4])?.parse::<u64>()?,
            ) == (
                self.pool_tokens[0].clone(),
                self.pool_tokens[1].clone(),
                3000,
            ),
            "official NPM positions call disagrees with the created pool",
        )?;
        self.check(
            "official token, Pool slot0 and NPM positions calls decode scalar and tuple ABI values",
        );
        let latest = self.rpc("eth_getBlockByNumber", json!(["latest", false]))?;
        let timestamp = hex_u64(&text(&latest["timestamp"])?)?;
        let next = (timestamp / 86_400 + 1) * 86_400 + 3_600;
        self.rpc("evm_setNextBlockTimestamp", json!([next]))?;
        self.pool_swap(Some(next + 3_600))?;
        self.check("official router swap executes after crossing a local day and hour boundary");
        let snapshot = self.rpc("evm_snapshot", json!([]))?;
        let orphan_pool = self.create_official_pool(500)?;
        self.pool_lifecycle()?;
        self.rpc("evm_mine", json!([]))?;
        let (orphan_height, orphan_hash) = self.head()?;
        require(
            self.rpc("evm_revert", json!([snapshot]))?.as_bool() == Some(true),
            "owned Anvil snapshot could not be reverted",
        )?;
        self.rpc("evm_mine", json!([]))?;
        let canonical_position = self.pool_lifecycle()?;
        self.rpc("evm_mine", json!([]))?;
        let (height, hash) = self.head()?;
        require(
            height == orphan_height && hash != orphan_hash,
            "official lifecycle fixture did not produce a same-height Anvil reorg",
        )?;
        let absent = text(&self.call(
            &self.pool_factory.clone(),
            "getPool(address,address,uint24)(address)",
            &[
                self.pool_tokens[0].clone(),
                self.pool_tokens[1].clone(),
                "500".into(),
            ],
        )?)?
        .to_lowercase();
        require(
            absent == "0x0000000000000000000000000000000000000000",
            "reverted official extra-fee pool still exists",
        )?;
        let owner = text(&self.call(
            &self.position_manager.clone(),
            "ownerOf(uint256)(address)",
            &[canonical_position],
        )?)?
        .to_lowercase();
        require(
            owner == self.accounts[2].to_lowercase(),
            "canonical reorg branch did not finish NPM lifecycle",
        )?;
        self.check("same-height reorg removes an official fee=500 pool and replays a separate NPM lifecycle");
        self.report["status"] = json!("passed");
        self.report["fixture"] = json!("official-uniswap-v3");
        self.report["factory"] = json!(self.pool_factory);
        self.report["pool"] = json!(pool);
        self.report["position_manager"] = json!(self.position_manager);
        self.report["swap_router"] = json!(self.swap_router);
        self.report["start_block"] = json!(start);
        self.report["orphan_pool"] = json!(orphan_pool);
        self.report["canonical_block"] = json!(height);
        self.report["mined_receipts"] = json!(self.receipts.len());
        Ok(())
    }

    /// Verifies indexed pool events and returns their normalized rows.
    pub(crate) fn pool_events(&mut self, schema: &str) -> Result<Vec<Value>> {
        let rows = self.entities(schema)?;
        let factory = rows.iter().find(|row| row[0] == "Factory");
        require(factory.is_some(), "Factory entity is missing")?;
        let factory = factory.expect("checked above")[2].clone();
        let mut expected = BTreeMap::new();
        for (event, signature) in [
            (
                "PoolCreated",
                "PoolCreated(address,address,uint24,int24,address)",
            ),
            (
                "Mint",
                "Mint(address,address,int24,int24,uint128,uint256,uint256)",
            ),
            ("Burn", "Burn(address,int24,int24,uint128,uint256,uint256)"),
            (
                "Swap",
                "Swap(address,address,int256,int256,uint160,uint128,int24)",
            ),
        ] {
            let address = if event == "PoolCreated" {
                self.pool_factory.clone()
            } else {
                self.pool_address.clone()
            };
            expected.insert(event, self.canonical_logs(&address, signature)?.len());
        }
        require(
            text(&factory["poolCount"])? == expected["PoolCreated"].to_string()
                && text(&factory["txCount"])?
                    == (expected["Mint"] + expected["Burn"] + expected["Swap"]).to_string(),
            "official V3 factory counters differ from canonical receipt logs",
        )?;
        for event in ["Mint", "Burn", "Swap"] {
            require(
                rows.iter().filter(|row| row[0] == event).count() == expected[event],
                format!("stored {event} count differs from canonical receipt logs"),
            )?;
        }
        let position_count = self
            .canonical_logs(
                &self.position_manager.clone(),
                "IncreaseLiquidity(uint256,uint128,uint256,uint256)",
            )?
            .len();
        require(
            rows.iter().filter(|row| row[0] == "Position").count() == position_count,
            "official position-manager lifecycle did not create positions",
        )?;
        let mut snapshot_ids = std::collections::BTreeSet::new();
        for (signature, token_topic) in [
            ("IncreaseLiquidity(uint256,uint128,uint256,uint256)", 1usize),
            ("DecreaseLiquidity(uint256,uint128,uint256,uint256)", 1),
            ("Collect(uint256,address,uint256,uint256)", 1),
            ("Transfer(address,address,uint256)", 3),
        ] {
            for log in self.canonical_logs(&self.position_manager.clone(), signature)? {
                snapshot_ids.insert(format!(
                    "{}#{}",
                    hex_bigint(&text(&log["topics"][token_topic])?)?,
                    hex_u64(&text(&log["blockNumber"])?)?
                ));
            }
        }
        let stored: std::collections::BTreeSet<_> = rows
            .iter()
            .filter(|row| row[0] == "PositionSnapshot")
            .map(|row| text(&row[1]))
            .collect::<Result<_>>()?;
        require(
            stored == snapshot_ids,
            "PositionSnapshot IDs differ from unique canonical NPM token/block pairs",
        )?;
        for row in rows.iter().filter(|row| row[0] == "Swap") {
            let amount0_negative = text(&row[2]["amount0"])?.starts_with('-');
            let amount1_negative = text(&row[2]["amount1"])?.starts_with('-');
            require(
                amount0_negative != amount1_negative,
                "official V3 Swap did not preserve signed token deltas",
            )?;
        }
        let mut ordered = Vec::new();
        for row in rows
            .iter()
            .filter(|row| row[0] == "Swap" || row[0] == "Mint")
        {
            let transaction = text(&row[2]["transaction"])?;
            let transaction_row = rows
                .iter()
                .find(|candidate| candidate[0] == "Transaction" && candidate[1] == transaction);
            require(
                transaction_row.is_some(),
                "event is missing its canonical Transaction",
            )?;
            let transaction_row = transaction_row.expect("checked above");
            ordered.push((
                text(&transaction_row[2]["blockNumber"])?.parse::<u64>()?,
                text(&row[2]["logIndex"])?.parse::<u64>()?,
                row,
            ));
        }
        ordered.sort_by_key(|(block_number, log_index, _)| (*block_number, *log_index));
        for (_, _, row) in ordered {
            let transaction = text(&row[2]["transaction"])?;
            require(
                text(&row[1])?.starts_with(&(transaction + "#")),
                "event ID is not derived from its canonical transaction",
            )?;
        }
        Ok(rows)
    }

    /// Loads pool-related relational rows keyed by table and identifier.
    pub(crate) fn pool_rows(&self, schema: &str) -> Result<BTreeMap<(String, String), Value>> {
        let mut rows = BTreeMap::new();
        for row in self.entities(schema)? {
            rows.insert((text(&row[0])?, text(&row[1])?), row[2].clone());
        }
        Ok(rows)
    }

    /// Compares indexed pool state with local contract calls and receipts.
    pub(crate) fn verify_pool_contract_state(&mut self, schema: &str, pool: &str) -> Result<()> {
        let rows = self.pool_rows(schema)?;
        let state = rows.get(&("Pool".into(), pool.into()));
        require(state.is_some(), "Pool entity is missing")?;
        let state = state.expect("checked above");
        let [token0, token1] = self.pool_tokens.clone();
        require(
            (
                text(&state["token0"])?,
                text(&state["token1"])?,
                text(&state["feeTier"])?,
            ) == (token0.clone(), token1.clone(), "3000".into()),
            "Pool metadata differs from the official Factory/Pool",
        )?;
        for token in [&token0, &token1] {
            let stored = rows.get(&("Token".into(), token.clone()));
            require(stored.is_some(), "Token entity is missing")?;
            let stored = stored.expect("checked above");
            require(
                (
                    text(&stored["name"])?,
                    text(&stored["symbol"])?,
                    text(&stored["decimals"])?,
                    text(&stored["totalSupply"])?,
                ) == (
                    text(&self.call(token, "name()(string)", &[])?)?,
                    text(&self.call(token, "symbol()(string)", &[])?)?,
                    text(&self.call(token, "decimals()(uint8)", &[])?)?
                        .parse::<u64>()?
                        .to_string(),
                    text(&self.call(token, "totalSupply()(uint256)", &[])?)?
                        .parse::<BigInt>()?
                        .to_string(),
                ),
                "Token metadata differs from its contract call",
            )?;
        }
        let slot0 = self.call(
            pool,
            "slot0()(uint160,int24,uint16,uint16,uint16,uint8,bool)",
            &[],
        )?;
        let slot0 = slot0.as_array();
        require(slot0.is_some(), "slot0 did not decode as tuple")?;
        let slot0 = slot0.expect("checked above");
        let liquidity = text(&self.call(pool, "liquidity()(uint128)", &[])?)?
            .parse::<BigInt>()?
            .to_string();
        let fee0 = text(&self.call(pool, "feeGrowthGlobal0X128()(uint256)", &[])?)?
            .parse::<BigInt>()?
            .to_string();
        let fee1 = text(&self.call(pool, "feeGrowthGlobal1X128()(uint256)", &[])?)?
            .parse::<BigInt>()?
            .to_string();
        require(
            (
                text(&state["sqrtPrice"])?,
                text(&state["tick"])?,
                text(&state["liquidity"])?,
                text(&state["feeGrowthGlobal0X128"])?,
                text(&state["feeGrowthGlobal1X128"])?,
            ) == (
                text(&slot0[0])?.parse::<BigInt>()?.to_string(),
                text(&slot0[1])?.parse::<BigInt>()?.to_string(),
                liquidity,
                fee0,
                fee1,
            ),
            "Pool state differs from official slot0/liquidity/fee-growth calls",
        )?;
        let sqrt_price = text(&slot0[0])?.parse::<BigInt>()?;
        let (price0, price1) = decimal34_price(&sqrt_price, 18, 18)?;
        require(
            (
                text(&state["token0Price"])?.parse::<BigDecimal>()?,
                text(&state["token1Price"])?.parse::<BigDecimal>()?,
            ) == (price0, price1),
            "Pool price differs from independent 34-digit sqrt-price arithmetic",
        )?;
        for tick in [-120, 120] {
            let values = self.call(
                pool,
                "ticks(int24)(uint128,int128,uint256,uint256,int56,uint160,uint32,bool)",
                &[tick.to_string()],
            )?;
            let values = values.as_array();
            require(values.is_some(), "ticks did not decode as tuple")?;
            let values = values.expect("checked above");
            let stored = rows.get(&("Tick".into(), format!("{pool}#{tick}")));
            require(stored.is_some(), "Tick entity is missing")?;
            let stored = stored.expect("checked above");
            require(
                (
                    text(&stored["liquidityGross"])?,
                    text(&stored["liquidityNet"])?,
                    text(&stored["feeGrowthOutside0X128"])?,
                    text(&stored["feeGrowthOutside1X128"])?,
                ) == (
                    text(&values[0])?.parse::<BigInt>()?.to_string(),
                    text(&values[1])?.parse::<BigInt>()?.to_string(),
                    text(&values[2])?.parse::<BigInt>()?.to_string(),
                    text(&values[3])?.parse::<BigInt>()?.to_string(),
                ),
                "Tick state differs from official pool.ticks",
            )?;
        }
        let position_manager = self.position_manager.clone();
        let owner = text(&self.call(
            &position_manager,
            "ownerOf(uint256)(address)",
            &["1".into()],
        )?)?
        .to_lowercase();
        let position = self.call(&position_manager, "positions(uint256)(uint96,address,address,address,uint24,int24,int24,uint128,uint256,uint256,uint128,uint128)", &["1".into()])?;
        let position = position.as_array();
        require(position.is_some(), "positions did not decode as tuple")?;
        let position = position.expect("checked above");
        let stored = rows.get(&("Position".into(), "1".into()));
        require(stored.is_some(), "Position entity is missing")?;
        let stored = stored.expect("checked above");
        require(
            (
                text(&stored["owner"])?,
                text(&stored["pool"])?,
                text(&stored["token0"])?,
                text(&stored["token1"])?,
                text(&stored["liquidity"])?,
                text(&stored["feeGrowthInside0LastX128"])?,
                text(&stored["feeGrowthInside1LastX128"])?,
            ) == (
                owner,
                pool.into(),
                token0,
                token1,
                text(&position[7])?.parse::<BigInt>()?.to_string(),
                text(&position[8])?.parse::<BigInt>()?.to_string(),
                text(&position[9])?.parse::<BigInt>()?.to_string(),
            ),
            "Position state differs from official NonfungiblePositionManager",
        )?;
        let position_manager = self.position_manager.clone();
        let increase = self.canonical_logs(
            &position_manager,
            "IncreaseLiquidity(uint256,uint128,uint256,uint256)",
        )?;
        let decrease = self.canonical_logs(
            &position_manager,
            "DecreaseLiquidity(uint256,uint128,uint256,uint256)",
        )?;
        let collect = self.canonical_logs(
            &position_manager,
            "Collect(uint256,address,uint256,uint256)",
        )?;
        require(
            increase.len() == 1 && decrease.len() == 1 && collect.len() == 1,
            "expected one canonical official NPM lifecycle",
        )?;
        let increase_words = log_words(&text(&increase[0]["data"])?)?;
        let decrease_words = log_words(&text(&decrease[0]["data"])?)?;
        let collect_words = log_words(&text(&collect[0]["data"])?)?;
        let scale = BigDecimal::from(BigInt::from(10u8).pow(18u32));
        require(
            (
                text(&stored["depositedToken0"])?.parse::<BigDecimal>()?,
                text(&stored["depositedToken1"])?.parse::<BigDecimal>()?,
                text(&stored["withdrawnToken0"])?.parse::<BigDecimal>()?,
                text(&stored["withdrawnToken1"])?.parse::<BigDecimal>()?,
            ) == (
                BigDecimal::from(increase_words[1].clone()) / &scale,
                BigDecimal::from(increase_words[2].clone()) / &scale,
                BigDecimal::from(decrease_words[1].clone()) / &scale,
                BigDecimal::from(decrease_words[2].clone()) / &scale,
            ),
            "Position deposits or withdrawals differ from NPM event amounts",
        )?;
        let expected_fee = BigDecimal::from(collect_words[1].clone()) / &scale;
        require(
            (
                text(&stored["collectedFeesToken0"])?.parse::<BigDecimal>()?,
                text(&stored["collectedFeesToken1"])?.parse::<BigDecimal>()?,
            ) == (expected_fee.clone(), expected_fee),
            "Position Collect does not retain the selected deployment behavior",
        )?;
        Ok(())
    }

    /// Returns normalized pool state visible at an indexed block height.
    pub(crate) fn pool_history(&self, schema: &str, height: u64) -> Result<Value> {
        let immutable = ["transaction", "mint", "burn", "swap", "collect", "flash"];
        let tables = [
            "factory",
            "bundle",
            "token",
            "pool",
            "tick",
            "transaction",
            "mint",
            "burn",
            "swap",
            "collect",
            "flash",
            "uniswap_day_data",
            "pool_day_data",
            "pool_hour_data",
            "token_day_data",
            "token_hour_data",
            "position",
            "position_snapshot",
            "tick_day_data",
            "tick_hour_data",
        ];
        let mut result = serde_json::Map::new();
        for table in tables {
            let predicate = if immutable.contains(&table) {
                format!("block_number <= {height}")
            } else {
                format!("block_range @> {height}::bigint")
            };
            let fields = if immutable.contains(&table) {
                "to_jsonb(t) - 'vid'"
            } else {
                "to_jsonb(t) - 'vid' - 'block_range'"
            };
            let output = self.sql(&format!(r#"SELECT COALESCE(jsonb_agg({fields} ORDER BY id), '[]'::jsonb) FROM {schema}."{table}" t WHERE {predicate};"#))?;
            let mut values = serde_json::from_str(&output)?;
            normalize_json_numbers(&mut values);
            result.insert(table.into(), values);
        }
        Ok(Value::Object(result))
    }

    /// Verifies a swap created the expected hourly and daily aggregates.
    pub(crate) fn verify_new_time_buckets(
        &self,
        schema: &str,
        pool: &str,
        timestamp: u64,
    ) -> Result<()> {
        let rows = self.pool_rows(schema)?;
        let state = rows.get(&("Pool".into(), pool.into()));
        require(state.is_some(), "Pool entity is missing")?;
        let state = state.expect("checked above");
        let (day, hour) = (timestamp / 86_400, timestamp / 3_600);
        let pool_day = rows.get(&("PoolDayData".into(), format!("{pool}-{day}")));
        let pool_hour = rows.get(&("PoolHourData".into(), format!("{pool}-{hour}")));
        require(pool_day.is_some(), "PoolDayData missing")?;
        require(pool_hour.is_some(), "PoolHourData missing")?;
        let pool_day = pool_day.expect("checked above");
        let pool_hour = pool_hour.expect("checked above");
        let price = text(&state["token0Price"])?.parse::<BigDecimal>()?;
        for bucket in [pool_day, pool_hour] {
            require(
                text(&bucket["open"])?.parse::<BigDecimal>()? == price
                    && text(&bucket["high"])?.parse::<BigDecimal>()? == price
                    && text(&bucket["low"])?.parse::<BigDecimal>()? == price
                    && text(&bucket["close"])?.parse::<BigDecimal>()? == price,
                "first official Swap in a new bucket did not initialize OHLC from current price",
            )?;
        }
        for token in &self.pool_tokens {
            require(
                rows.contains_key(&("TokenDayData".into(), format!("{token}-{day}")))
                    && rows.contains_key(&("TokenHourData".into(), format!("{token}-{hour}"))),
                "official Swap did not create both token day/hour buckets",
            )?;
        }
        require(
            rows.contains_key(&("UniswapDayData".into(), day.to_string())),
            "official Swap did not create the day aggregate",
        )
    }

    /// Creates a pool through the pinned official factory fixture.
    pub(crate) fn create_official_pool(&mut self, fee: u32) -> Result<String> {
        let [token0, token1] = self.pool_tokens.clone();
        let manager = self.position_manager.clone();
        self.send(
            &manager,
            "createAndInitializePoolIfNecessary(address,address,uint24,uint160)",
            &[
                token0.clone(),
                token1.clone(),
                fee.to_string(),
                "79228162514264337593543950336".into(),
            ],
            0,
        )?;
        let pool = text(&self.call(
            &self.pool_factory.clone(),
            "getPool(address,address,uint24)(address)",
            &[token0, token1, fee.to_string()],
        )?)?
        .to_lowercase();
        require(
            pool != "0x0000000000000000000000000000000000000000",
            "official factory did not create extra fee pool",
        )?;
        Ok(pool)
    }

    /// Executes a fixture router swap and returns its receipt.
    pub(crate) fn pool_swap(&mut self, deadline: Option<u64>) -> Result<Value> {
        let [token0, token1] = self.pool_tokens.clone();
        let deadline = match deadline {
            Some(value) => value,
            None => {
                hex_u64(&text(
                    &self.rpc("eth_getBlockByNumber", json!(["latest", false]))?["timestamp"],
                )?)? + 3600
            }
        };
        let router = self.swap_router.clone();
        self.send(
            &router,
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
            &[format!(
                "({token0},{token1},3000,{},{deadline},1000000000000000000,0,0)",
                self.accounts[1]
            )],
            1,
        )
    }

    /// Returns pool version rows used by reorganization assertions.
    pub(crate) fn pool_versions(&self, schema: &str) -> Result<Value> {
        let immutable = ["transaction", "mint", "burn", "swap", "collect", "flash"];
        let tables = [
            "factory",
            "bundle",
            "token",
            "pool",
            "tick",
            "transaction",
            "mint",
            "burn",
            "swap",
            "collect",
            "flash",
            "uniswap_day_data",
            "pool_day_data",
            "pool_hour_data",
            "token_day_data",
            "token_hour_data",
            "position",
            "position_snapshot",
            "tick_day_data",
            "tick_hour_data",
        ];
        let mut result = serde_json::Map::new();
        for table in tables {
            let ordering = if immutable.contains(&table) {
                "id, block_number"
            } else {
                "id, block_range"
            };
            let output = self.sql(&format!(r#"SELECT COALESCE(jsonb_agg(to_jsonb(t) - 'vid' ORDER BY {ordering}), '[]'::jsonb) FROM {schema}."{table}" t;"#))?;
            let mut values = serde_json::from_str(&output)?;
            normalize_json_numbers(&mut values);
            result.insert(table.into(), values);
        }
        Ok(Value::Object(result))
    }
}

//! Rust mapping for the 20-entity Uniswap V3 index.
use crate::{
    ExampleError,
    chain::ChainConfig,
    entities::{
        Bundle, Burn as BurnEntity, Factory, Mint as MintEntity, Pool, Swap as SwapEntity, Tick,
        Token, Transaction,
    },
    events::*,
    math::{decimal, integer, power, round34, units},
};
use alloy_primitives::{Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::{Block, TransactionRequest};
use alloy_sol_types::{SolCall, SolEvent};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use raven_engine::{
    EngineError, Entity, EntityStore, EntityStoreExt, Handler, RavenError, RavenResult,
};
use raven_evm::{LogUpdate, Parsed};
use std::{future::IntoFuture, sync::Arc, time::Duration};
use tokio::sync::Mutex;

/// Loads a required entity or reports invalid pool state.
pub(crate) async fn load<T: Entity>(store: &mut dyn EntityStore, id: &str) -> RavenResult<T> {
    store
        .load::<T>(id)
        .await?
        .ok_or_else(|| ExampleError::InvalidPoolState.into())
}
pub(crate) struct Context {
    pub timestamp: u64,
    pub number: u64,
    pub tx: String,
    pub log_index: u64,
    pub origin: String,
    pub gas_price: BigDecimal,
}

enum LiquidityKind {
    Mint,
    Burn,
}

struct LiquidityEventInput {
    kind: LiquidityKind,
    owner: Address,
    sender: Option<Address>,
    lower: i32,
    upper: i32,
    amount: String,
    raw0: String,
    raw1: String,
}

pub struct Mapping<P = RootProvider> {
    pub(crate) provider: Arc<P>,
    pub(crate) factory: String,
    pub(crate) position_manager: Address,
    chain: ChainConfig,
    // Cache only immutable RPC payload for the current hash, never entity state.
    block: Mutex<Option<(B256, Block)>>,
}

impl<P: Provider> Mapping<P> {
    /// Creates a mapping with its immutable chain configuration.
    pub fn new(
        provider: Arc<P>,
        factory: Address,
        position_manager: Address,
        chain: ChainConfig,
    ) -> Self {
        Self {
            provider,
            factory: factory.to_string().to_lowercase(),
            position_manager,
            chain,
            block: Mutex::new(None),
        }
    }

    /// Executes a contract call at a block hash, treating reverts as absent data.
    pub(crate) async fn call<C: SolCall>(
        &self,
        address: Address,
        hash: B256,
        call: C,
    ) -> RavenResult<Option<C::Return>> {
        let tx = TransactionRequest::default()
            .to(address)
            .input(call.abi_encode().into());
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            self.provider
                .call(tx)
                .block((hash, Some(false)).into())
                .into_future(),
        )
        .await
        .map_err(|_| ExampleError::RequestTimeout)?;
        match result {
            Ok(output) => Ok(C::abi_decode_returns_validate(&output).ok()),
            Err(error)
                if error.as_error_resp().is_some_and(|r| {
                    r.code == 3 || r.message.to_lowercase().contains("revert")
                }) =>
            {
                Ok(None)
            }
            Err(error) => Err(RavenError::Source(Box::new(error))),
        }
    }

    /// Resolves the block and transaction metadata required by an event.
    pub(crate) async fn context<T>(&self, event: &Parsed<T>) -> RavenResult<Context> {
        let transaction_hash = event
            .transaction_hash
            .ok_or(ExampleError::MissingMetadata)?;
        let mut cached = self.block.lock().await;
        if cached.as_ref().map(|(hash, _)| hash) != Some(&event.block_hash) {
            let block = tokio::time::timeout(
                Duration::from_secs(15),
                self.provider
                    .get_block_by_hash(event.block_hash)
                    .full()
                    .into_future(),
            )
            .await
            .map_err(|_| ExampleError::RequestTimeout)?
            .map_err(|error| RavenError::Source(Box::new(error)))?
            .ok_or(EngineError::MissingBlock)?;
            if block.header.hash != event.block_hash
                || block.header.inner.number != event.block_number
            {
                return Err(EngineError::InvalidBlock.into());
            }
            *cached = Some((event.block_hash, block));
        }
        let block = &cached.as_ref().ok_or(EngineError::MissingBlock)?.1;
        let transaction = block
            .transactions
            .as_transactions()
            .ok_or(ExampleError::MissingMetadata)?
            .iter()
            .find(|transaction| *transaction.inner.tx_hash() == transaction_hash)
            .ok_or(ExampleError::MissingMetadata)?;
        Ok(Context {
            timestamp: block.header.inner.timestamp,
            number: event.block_number,
            tx: transaction_hash.to_string(),
            log_index: event.log_index.ok_or(ExampleError::MissingMetadata)?,
            origin: transaction.inner.signer().to_string().to_lowercase(),
            gas_price: integer(
                &transaction
                    .effective_gas_price
                    .ok_or(ExampleError::MissingMetadata)?
                    .to_string(),
            ),
        })
    }

    /// Loads token metadata, fetching it from the contract when first observed.
    async fn token(
        &self,
        store: &mut dyn EntityStore,
        address: Address,
        hash: B256,
    ) -> RavenResult<Option<Token>> {
        let id = address.to_string().to_lowercase();
        if let Some(token) = store.load::<Token>(&id).await? {
            return Ok(Some(token));
        }
        let mut token = Token {
            id: id.clone(),
            ..Token::default()
        };
        let definition = self.chain.tokens.get(&id);
        let symbol = self.call(address, hash, TokenMetadata::symbolCall).await?;
        let name = self.call(address, hash, TokenMetadata::nameCall).await?;
        for (is_symbol, field, string) in [
            (true, &mut token.symbol, symbol),
            (false, &mut token.name, name),
        ] {
            *field = if let Some(text) = string {
                text
            } else {
                let bytes = if is_symbol {
                    self.call(address, hash, BytesMetadata::symbolCall).await?
                } else {
                    self.call(address, hash, BytesMetadata::nameCall).await?
                };
                if bytes.is_some_and(|value| {
                    value.as_slice()[..31].iter().all(|b| *b == 0) && value.as_slice()[31] == 1
                }) {
                    definition
                        .map(|value| {
                            if is_symbol {
                                value.symbol.clone()
                            } else {
                                value.name.clone()
                            }
                        })
                        .unwrap_or_else(|| "unknown".into())
                } else {
                    bytes_metadata(bytes)
                }
            };
        }
        token.total_supply = integer(
            &self
                .call(address, hash, TokenMetadata::totalSupplyCall)
                .await?
                .unwrap_or_default()
                .to_string(),
        );
        let decimals = match self
            .call(address, hash, TokenMetadata::decimalsCall)
            .await?
        {
            Some(value) if value < 255 => Some(value),
            Some(_) => None,
            None => definition.map(|value| u32::from(value.decimals)),
        };
        let Some(decimals) = decimals else {
            return Ok(None);
        };
        token.decimals = BigDecimal::from(decimals);
        Ok(Some(token))
    }

    /// Records the transaction metadata associated with an indexed event.
    pub(crate) async fn transaction(
        &self,
        store: &mut dyn EntityStore,
        context: &Context,
    ) -> RavenResult<()> {
        let tx = Transaction {
            id: context.tx.clone(),
            block_number: BigDecimal::from(context.number),
            timestamp: BigDecimal::from(context.timestamp),
            gas_price: context.gas_price.clone(),
            // Store zero gas_used; receipt gas is not part of this mapping.
            ..Transaction::default()
        };
        store.save(&tx).await
    }

    /// Derives the reference asset's USD price from the configured stable pool.
    async fn eth_price(&self, store: &mut dyn EntityStore) -> RavenResult<BigDecimal> {
        if self.chain.stable_token_pool == self.chain.reference_token {
            return Ok(BigDecimal::from(1));
        }
        let Some(pool) = store.load::<Pool>(&self.chain.stable_token_pool).await? else {
            return Ok(BigDecimal::from(0));
        };
        Ok(if pool.token0 == self.chain.reference_token {
            pool.token1_price
        } else {
            pool.token0_price
        })
    }

    /// Derives a token's reference-asset price from eligible liquid pools.
    async fn derived(&self, store: &mut dyn EntityStore, token: &Token) -> RavenResult<BigDecimal> {
        if token.id == self.chain.reference_token {
            return Ok(BigDecimal::from(1));
        }
        let bundle: Bundle = load(store, "1").await?;
        if self.chain.stable_coins.contains(&token.id) {
            return Ok(if bundle.eth_price_usd == 0 {
                BigDecimal::from(0)
            } else {
                round34(BigDecimal::from(1) / bundle.eth_price_usd)
            });
        }
        let mut largest = BigDecimal::from(0);
        let mut price = BigDecimal::from(0);
        for id in &token.whitelist_pools {
            let Some(pool) = store.load::<Pool>(id).await? else {
                continue;
            };
            if pool.liquidity <= 0 {
                continue;
            }
            let (other_id, locked, pool_price) = if pool.token0 == token.id {
                (
                    &pool.token1,
                    &pool.total_value_locked_token1,
                    &pool.token1_price,
                )
            } else {
                (
                    &pool.token0,
                    &pool.total_value_locked_token0,
                    &pool.token0_price,
                )
            };
            let Some(other) = store.load::<Token>(other_id).await? else {
                continue;
            };
            let locked = round34(locked.clone() * other.derived_eth.clone());
            if locked > largest && locked > decimal(&self.chain.minimum_native_locked) {
                largest = locked;
                price = round34(pool_price.clone() * other.derived_eth);
            }
        }
        Ok(price)
    }

    /// Calculates tracked USD volume using the configured whitelist policy.
    fn tracked(
        &self,
        amount0: BigDecimal,
        token0: &Token,
        amount1: BigDecimal,
        token1: &Token,
        bundle: &Bundle,
    ) -> BigDecimal {
        let price0 = round34(token0.derived_eth.clone() * bundle.eth_price_usd.clone());
        let price1 = round34(token1.derived_eth.clone() * bundle.eth_price_usd.clone());
        let white0 = self.chain.whitelist_tokens.contains(&token0.id);
        let white1 = self.chain.whitelist_tokens.contains(&token1.id);
        match (white0, white1) {
            (true, true) => round34(round34(amount0 * price0) + round34(amount1 * price1)),
            (true, false) => round34(round34(amount0 * price0) * decimal("2")),
            (false, true) => round34(round34(amount1 * price1) * decimal("2")),
            _ => BigDecimal::from(0),
        }
    }

    /// Adds a pool to the counterpart token's price-discovery candidates.
    fn link_tokens(&self, pool: &str, token0: &mut Token, token1: &mut Token) {
        if self.chain.whitelist_tokens.contains(&token0.id) {
            token1.whitelist_pools.push(pool.into());
        }
        if self.chain.whitelist_tokens.contains(&token1.id) {
            token0.whitelist_pools.push(pool.into());
        }
    }

    /// Creates pool, token, factory, and bundle state for a PoolCreated event.
    async fn created(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<PoolCreated>,
    ) -> RavenResult<()> {
        let id = event.value.pool.to_string().to_lowercase();
        if self.chain.skip_pools.contains(&id) {
            return Ok(());
        }
        let context = self.context(event).await?;
        let mut factory = match store.load::<Factory>(&self.factory).await? {
            Some(factory) => factory,
            None => {
                let factory = Factory {
                    id: self.factory.clone(),
                    owner: Address::ZERO.to_string().to_lowercase(),
                    ..Factory::default()
                };
                let bundle = Bundle {
                    id: "1".into(),
                    ..Bundle::default()
                };
                store.save(&bundle).await?;
                factory
            }
        };
        factory.pool_count += BigDecimal::from(1);
        let Some(mut token0) = self
            .token(store, event.value.token0, event.block_hash)
            .await?
        else {
            return Ok(());
        };
        let Some(mut token1) = self
            .token(store, event.value.token1, event.block_hash)
            .await?
        else {
            return Ok(());
        };
        self.link_tokens(&id, &mut token0, &mut token1);
        let pool = Pool {
            id,
            token0: token0.id.clone(),
            token1: token1.id.clone(),
            fee_tier: integer(&event.value.fee.to_string()),
            created_at_timestamp: BigDecimal::from(context.timestamp),
            created_at_block_number: BigDecimal::from(context.number),
            ..Pool::default()
        };
        store.save(&pool).await?;
        store.save(&token0).await?;
        store.save(&token1).await?;
        store.save(&factory).await
    }

    /// Applies initial pool price state and refreshes dependent prices.
    async fn initialized(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<Initialize>,
    ) -> RavenResult<()> {
        let id = event
            .address
            .ok_or(ExampleError::MissingMetadata)?
            .to_string()
            .to_lowercase();
        let Some(mut pool) = store.load::<Pool>(&id).await? else {
            return Ok(());
        };
        let context = self.context(event).await?;
        pool.sqrt_price = integer(&event.value.sqrtPriceX96.to_string());
        pool.tick = Some(integer(&event.value.tick.to_string()));
        store.save(&pool).await?;
        let mut bundle: Bundle = load(store, "1").await?;
        bundle.eth_price_usd = self.eth_price(store).await?;
        store.save(&bundle).await?;
        crate::intervals::pool_intervals(store, &context, &pool, None).await?;
        let mut token0: Token = load(store, &pool.token0).await?;
        let mut token1: Token = load(store, &pool.token1).await?;
        let price0 = self.derived(store, &token0).await?;
        let price1 = self.derived(store, &token1).await?;
        token0.derived_eth = price0;
        token1.derived_eth = price1;
        store.save(&token0).await?;
        store.save(&token1).await
    }

    /// Recomputes pool and factory total value locked from token balances.
    fn tvl(
        &self,
        pool: &mut Pool,
        token0: &Token,
        token1: &Token,
        factory: &mut Factory,
        bundle: &Bundle,
    ) {
        let eth = round34(
            round34(pool.total_value_locked_token0.clone() * token0.derived_eth.clone())
                + round34(pool.total_value_locked_token1.clone() * token1.derived_eth.clone()),
        );
        pool.total_value_locked_eth = eth.clone();
        pool.total_value_locked_usd = round34(eth.clone() * bundle.eth_price_usd.clone());
        factory.total_value_locked_eth = round34(factory.total_value_locked_eth.clone() + eth);
        factory.total_value_locked_usd =
            round34(factory.total_value_locked_eth.clone() * bundle.eth_price_usd.clone());
    }

    /// Applies common Mint or Burn accounting and persists its event entity.
    async fn liquidity_event<T>(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<T>,
        input: LiquidityEventInput,
    ) -> RavenResult<()> {
        let LiquidityEventInput {
            kind,
            owner,
            sender,
            lower,
            upper,
            amount,
            raw0,
            raw1,
        } = input;
        let id = event
            .address
            .ok_or(ExampleError::MissingMetadata)?
            .to_string()
            .to_lowercase();
        let Some(mut pool) = store.load::<Pool>(&id).await? else {
            return Ok(());
        };
        let context = self.context(event).await?;
        let bundle: Bundle = load(store, "1").await?;
        let mut factory: Factory = load(store, &self.factory).await?;
        let mut token0: Token = load(store, &pool.token0).await?;
        let mut token1: Token = load(store, &pool.token1).await?;
        let amount0 = units(&raw0, &token0.decimals.to_string());
        let amount1 = units(&raw1, &token1.decimals.to_string());
        let usd = round34(
            round34(
                amount0.clone()
                    * round34(token0.derived_eth.clone() * bundle.eth_price_usd.clone()),
            ) + round34(
                amount1.clone()
                    * round34(token1.derived_eth.clone() * bundle.eth_price_usd.clone()),
            ),
        );
        let mint = matches!(kind, LiquidityKind::Mint);
        factory.total_value_locked_eth =
            round34(factory.total_value_locked_eth.clone() - pool.total_value_locked_eth.clone());
        let delta0 = if mint {
            amount0.clone()
        } else {
            round34(BigDecimal::from(0) - amount0.clone())
        };
        let delta1 = if mint {
            amount1.clone()
        } else {
            round34(BigDecimal::from(0) - amount1.clone())
        };
        token0.total_value_locked = round34(token0.total_value_locked.clone() + delta0.clone());
        token1.total_value_locked = round34(token1.total_value_locked.clone() + delta1.clone());
        // Mint/Burn round the USD price before multiplying by the balance.
        for token in [&mut token0, &mut token1] {
            token.total_value_locked_usd = round34(
                token.total_value_locked.clone()
                    * round34(token.derived_eth.clone() * bundle.eth_price_usd.clone()),
            );
        }
        pool.total_value_locked_token0 = round34(pool.total_value_locked_token0.clone() + delta0);
        pool.total_value_locked_token1 = round34(pool.total_value_locked_token1.clone() + delta1);
        factory.tx_count += BigDecimal::from(1);
        pool.tx_count += BigDecimal::from(1);
        token0.tx_count += BigDecimal::from(1);
        token1.tx_count += BigDecimal::from(1);
        if let Some(tick) = &pool.tick
            && tick >= &BigDecimal::from(lower)
            && tick < &BigDecimal::from(upper)
        {
            pool.liquidity += if mint {
                integer(&amount)
            } else {
                -integer(&amount)
            };
        }
        self.tvl(&mut pool, &token0, &token1, &mut factory, &bundle);
        self.transaction(store, &context).await?;
        let entity_id = format!("{}#{}", context.tx, pool.tx_count);
        let lower_id = format!("{id}#{lower}");
        let upper_id = format!("{id}#{upper}");
        let mut lower_tick = store.load::<Tick>(&lower_id).await?;
        let mut upper_tick = store.load::<Tick>(&upper_id).await?;
        if mint {
            if lower_tick.is_none() {
                lower_tick = Some(new_tick(&id, lower, &context));
            }
            if upper_tick.is_none() {
                upper_tick = Some(new_tick(&id, upper, &context));
            }
        }
        let mut lower_tick_value = None;
        let mut upper_tick_value = None;
        if let (Some(mut lower_tick), Some(mut upper_tick)) = (lower_tick, upper_tick) {
            let delta = if mint {
                integer(&amount)
            } else {
                -integer(&amount)
            };
            lower_tick.liquidity_gross += delta.clone();
            lower_tick.liquidity_net += delta.clone();
            upper_tick.liquidity_gross += delta.clone();
            upper_tick.liquidity_net -= delta;
            if !mint {
                self.tick_fee_vars(store, &mut lower_tick, event, &context)
                    .await?;
                self.tick_fee_vars(store, &mut upper_tick, event, &context)
                    .await?;
            }
            lower_tick_value = Some(lower_tick);
            upper_tick_value = Some(upper_tick);
        }
        // Update intervals before persisting the entities they read.
        crate::intervals::update(store, &context, &self.factory, &id, &token0, &token1, None)
            .await?;
        store.save(&token0).await?;
        store.save(&token1).await?;
        store.save(&pool).await?;
        store.save(&factory).await?;
        if mint {
            let entity = MintEntity {
                id: entity_id,
                transaction: context.tx.clone(),
                timestamp: BigDecimal::from(context.timestamp),
                pool: id.clone(),
                token0: pool.token0.clone(),
                token1: pool.token1.clone(),
                owner: owner.to_string().to_lowercase(),
                sender: sender.map(|sender| sender.to_string().to_lowercase()),
                origin: context.origin.clone(),
                amount: integer(&amount),
                amount0,
                amount1,
                amount_usd: Some(usd),
                tick_lower: BigDecimal::from(lower),
                tick_upper: BigDecimal::from(upper),
                log_index: Some(BigDecimal::from(context.log_index)),
            };
            store.save(&entity).await?;
        } else {
            let entity = BurnEntity {
                id: entity_id,
                transaction: context.tx.clone(),
                timestamp: BigDecimal::from(context.timestamp),
                pool: id.clone(),
                token0: pool.token0.clone(),
                token1: pool.token1.clone(),
                owner: Some(owner.to_string().to_lowercase()),
                origin: context.origin.clone(),
                amount: integer(&amount),
                amount0,
                amount1,
                amount_usd: Some(usd),
                tick_lower: BigDecimal::from(lower),
                tick_upper: BigDecimal::from(upper),
                log_index: Some(BigDecimal::from(context.log_index)),
            };
            store.save(&entity).await?;
        }
        if mint {
            if let Some(mut tick) = lower_tick_value {
                self.tick_fee_vars(store, &mut tick, event, &context)
                    .await?;
            }
            if let Some(mut tick) = upper_tick_value {
                self.tick_fee_vars(store, &mut tick, event, &context)
                    .await?;
            }
        }
        Ok(())
    }

    /// Applies swap volume, pricing, liquidity, and interval accounting.
    async fn swapped(&self, store: &mut dyn EntityStore, event: &Parsed<Swap>) -> RavenResult<()> {
        let id = event
            .address
            .ok_or(ExampleError::MissingMetadata)?
            .to_string()
            .to_lowercase();
        // This pool is explicitly excluded from swap pricing.
        if id == "0x9663f2ca0454accad3e094448ea6f77443880454" {
            return Ok(());
        }
        let Some(mut pool) = store.load::<Pool>(&id).await? else {
            return Ok(());
        };
        let old_tick = pool.tick.clone();
        let context = self.context(event).await?;
        let mut bundle: Bundle = load(store, "1").await?;
        let mut factory: Factory = load(store, &self.factory).await?;
        let mut token0: Token = load(store, &pool.token0).await?;
        let mut token1: Token = load(store, &pool.token1).await?;
        let amount0 = units(
            &event.value.amount0.to_string(),
            &token0.decimals.to_string(),
        );
        let amount1 = units(
            &event.value.amount1.to_string(),
            &token1.decimals.to_string(),
        );
        let abs0 = amount0.abs();
        let abs1 = amount1.abs();
        let eth0 = round34(abs0.clone() * token0.derived_eth.clone());
        let eth1 = round34(abs1.clone() * token1.derived_eth.clone());
        let untracked = round34(
            round34(
                round34(eth0 * bundle.eth_price_usd.clone())
                    + round34(eth1 * bundle.eth_price_usd.clone()),
            ) / decimal("2"),
        );
        let tracked = round34(
            self.tracked(abs0.clone(), &token0, abs1.clone(), &token1, &bundle) / decimal("2"),
        );
        let eth = if bundle.eth_price_usd == 0 {
            BigDecimal::from(0)
        } else {
            round34(tracked.clone() / bundle.eth_price_usd.clone())
        };
        let fee_tier = round34(pool.fee_tier.clone());
        let fees_eth = round34(round34(eth.clone() * fee_tier.clone()) / decimal("1000000"));
        let fees = round34(round34(tracked.clone() * fee_tier) / decimal("1000000"));
        factory.tx_count += BigDecimal::from(1);
        factory.total_volume_eth = round34(factory.total_volume_eth.clone() + eth.clone());
        factory.total_volume_usd = round34(factory.total_volume_usd.clone() + tracked.clone());
        factory.untracked_volume_usd =
            round34(factory.untracked_volume_usd.clone() + untracked.clone());
        factory.total_fees_eth = round34(factory.total_fees_eth.clone() + fees_eth);
        factory.total_fees_usd = round34(factory.total_fees_usd.clone() + fees.clone());
        factory.total_value_locked_eth =
            round34(factory.total_value_locked_eth.clone() - pool.total_value_locked_eth.clone());
        pool.tx_count += BigDecimal::from(1);
        pool.volume_token0 = round34(pool.volume_token0.clone() + abs0.clone());
        pool.volume_token1 = round34(pool.volume_token1.clone() + abs1.clone());
        pool.volume_usd = round34(pool.volume_usd.clone() + tracked.clone());
        pool.untracked_volume_usd = round34(pool.untracked_volume_usd.clone() + untracked.clone());
        pool.fees_usd = round34(pool.fees_usd.clone() + fees.clone());
        pool.liquidity = integer(&event.value.liquidity.to_string());
        pool.tick = Some(integer(&event.value.tick.to_string()));
        pool.sqrt_price = integer(&event.value.sqrtPriceX96.to_string());
        pool.total_value_locked_token0 =
            round34(pool.total_value_locked_token0.clone() + amount0.clone());
        pool.total_value_locked_token1 =
            round34(pool.total_value_locked_token1.clone() + amount1.clone());
        for (token, amount, abs) in [
            (&mut token0, amount0.clone(), abs0.clone()),
            (&mut token1, amount1.clone(), abs1.clone()),
        ] {
            token.tx_count += BigDecimal::from(1);
            token.volume = round34(token.volume.clone() + abs);
            token.total_value_locked = round34(token.total_value_locked.clone() + amount);
            token.volume_usd = round34(token.volume_usd.clone() + tracked.clone());
            token.untracked_volume_usd =
                round34(token.untracked_volume_usd.clone() + untracked.clone());
            token.fees_usd = round34(token.fees_usd.clone() + fees.clone());
        }
        let sqrt = integer(&event.value.sqrtPriceX96.to_string());
        let q192 = decimal(&(alloy_primitives::U256::from(1) << 192_usize).to_string());
        let scale0 = decimal(&format!("1e{}", token0.decimals));
        let scale1 = decimal(&format!("1e{}", token1.decimals));
        let ratio = round34(sqrt.square());
        let ratio = round34(ratio / q192);
        let ratio = round34(ratio * scale0);
        let ratio = round34(ratio / scale1);
        pool.token0_price = if ratio == 0 {
            BigDecimal::from(0)
        } else {
            round34(BigDecimal::from(1) / ratio.clone())
        };
        pool.token1_price = ratio;
        store.save(&pool).await?;
        bundle.eth_price_usd = self.eth_price(store).await?;
        store.save(&bundle).await?;
        let price0 = self.derived(store, &token0).await?;
        let price1 = self.derived(store, &token1).await?;
        token0.derived_eth = price0;
        token1.derived_eth = price1;
        self.tvl(&mut pool, &token0, &token1, &mut factory, &bundle);
        // Swap instead rounds balance * derivedETH before multiplying by USD.
        for token in [&mut token0, &mut token1] {
            token.total_value_locked_usd = round34(
                round34(token.total_value_locked.clone() * token.derived_eth.clone())
                    * bundle.eth_price_usd.clone(),
            );
        }
        self.transaction(store, &context).await?;
        let swap = SwapEntity {
            id: format!("{}#{}", context.tx, pool.tx_count),
            transaction: context.tx.clone(),
            timestamp: BigDecimal::from(context.timestamp),
            pool: id.clone(),
            token0: pool.token0.clone(),
            token1: pool.token1.clone(),
            sender: event.value.sender.to_string().to_lowercase(),
            recipient: event.value.recipient.to_string().to_lowercase(),
            origin: context.origin.clone(),
            amount0,
            amount1,
            amount_usd: tracked.clone(),
            sqrt_price_x96: integer(&event.value.sqrtPriceX96.to_string()),
            tick: integer(&event.value.tick.to_string()),
            log_index: Some(BigDecimal::from(context.log_index)),
        };
        self.pool_fee_vars(&mut pool, event).await?;
        let metrics = crate::intervals::Metrics {
            amount0: abs0,
            amount1: abs1,
            usd: tracked,
            eth,
            fees,
        };
        crate::intervals::update(
            store,
            &context,
            &self.factory,
            &id,
            &token0,
            &token1,
            Some(&metrics),
        )
        .await?;
        store.save(&swap).await?;
        store.save(&factory).await?;
        store.save(&pool).await?;
        store.save(&token0).await?;
        store.save(&token1).await?;
        self.crossed_ticks(store, &pool, old_tick, event, &context)
            .await
    }
}

/// Converts an optional bytes32 metadata response into display text.
fn bytes_metadata(value: Option<alloy_primitives::B256>) -> String {
    let Some(value) = value else {
        return "unknown".into();
    };
    if value.as_slice()[..31].iter().all(|byte| *byte == 0) && value.as_slice()[31] == 1 {
        return "unknown".into();
    }
    String::from_utf8_lossy(value.as_slice())
        .trim_end_matches('\0')
        .to_owned()
}

/// Creates the initial tick record for a pool index.
fn new_tick(pool: &str, index: i32, context: &Context) -> Tick {
    let price = power(&decimal("1.0001"), index);
    Tick {
        id: format!("{pool}#{index}"),
        pool: pool.into(),
        pool_address: Some(pool.into()),
        tick_idx: BigDecimal::from(index),
        created_at_timestamp: BigDecimal::from(context.timestamp),
        created_at_block_number: BigDecimal::from(context.number),
        price0: price.clone(),
        price1: round34(BigDecimal::from(1) / price),
        ..Tick::default()
    }
}

/// Membership is read through EntityStore before ABI decoding. New pools from
/// earlier logs are immediately visible; malformed logs from unrelated contracts
/// cannot stop an index that does not track them.
pub struct MappingHandler<P = RootProvider>(pub Arc<Mapping<P>>);
#[async_trait]
impl<P: Provider> Handler<LogUpdate> for MappingHandler<P> {
    /// Dispatches a decoded pool or position-manager log to its accounting handler.
    async fn handle(&self, store: &mut dyn EntityStore, log: &LogUpdate) -> RavenResult<()> {
        let Some(signature) = log.log.topics().first() else {
            return Ok(());
        };
        if signature == &PoolCreated::SIGNATURE_HASH {
            if log.log.address.to_string().to_lowercase() != self.0.factory {
                return Ok(());
            }
            let value = PoolCreated::decode_log_validate(&log.log)
                .map_err(|error| RavenError::Parser(Box::new(error)))?;
            return self.0.created(store, &log.parsed(value.data)).await;
        }
        if log.log.address == self.0.position_manager {
            return self.0.position_event(store, log).await;
        }
        let id = log.log.address.to_string().to_lowercase();
        if store.get(Pool::ENTITY_NAME, &id).await?.is_none() {
            return Ok(());
        }
        macro_rules! decode {
            ($event:ty) => {
                log.parsed(
                    <$event>::decode_log_validate(&log.log)
                        .map_err(|error| RavenError::Parser(Box::new(error)))?
                        .data,
                )
            };
        }
        if signature == &Initialize::SIGNATURE_HASH {
            self.0.initialized(store, &decode!(Initialize)).await
        } else if signature == &Swap::SIGNATURE_HASH {
            self.0.swapped(store, &decode!(Swap)).await
        } else if signature == &Flash::SIGNATURE_HASH {
            self.0.flashed(store, &decode!(Flash)).await
        } else if signature == &Mint::SIGNATURE_HASH {
            let event = decode!(Mint);
            let v = &event.value;
            self.0
                .liquidity_event(
                    store,
                    &event,
                    LiquidityEventInput {
                        kind: LiquidityKind::Mint,
                        owner: v.owner,
                        sender: Some(v.sender),
                        lower: v.tickLower.as_i32(),
                        upper: v.tickUpper.as_i32(),
                        amount: v.amount.to_string(),
                        raw0: v.amount0.to_string(),
                        raw1: v.amount1.to_string(),
                    },
                )
                .await
        } else if signature == &Burn::SIGNATURE_HASH {
            let event = decode!(Burn);
            let v = &event.value;
            self.0
                .liquidity_event(
                    store,
                    &event,
                    LiquidityEventInput {
                        kind: LiquidityKind::Burn,
                        owner: v.owner,
                        sender: None,
                        lower: v.tickLower.as_i32(),
                        upper: v.tickUpper.as_i32(),
                        amount: v.amount.to_string(),
                        raw0: v.amount0.to_string(),
                        raw1: v.amount1.to_string(),
                    },
                )
                .await
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

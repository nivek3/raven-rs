//! NFT mappings from the configured NonfungiblePositionManager.
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_sol_types::SolEvent;
use raven_engine::{EntityStore, EntityStoreExt, RavenError, RavenResult};
use raven_evm::{LogUpdate, Parsed};

use crate::{
    ExampleError,
    entities::{Position, PositionSnapshot, Token},
    events::{FactoryMetadata, PositionManager},
    handlers::{Context, Mapping},
    math::{integer, round34, units},
};

impl<P: Provider> Mapping<P> {
    /// Loads the on-chain position and builds its current indexed representation.
    async fn position<T>(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<T>,
        token_id: U256,
        context: &Context,
    ) -> RavenResult<Option<Position>> {
        let id = token_id.to_string();
        if let Some(position) = store.load::<Position>(&id).await? {
            return Ok(Some(position));
        }
        let Some(result) = self
            .call(
                self.position_manager,
                event.block_hash,
                PositionManager::positionsCall { tokenId: token_id },
            )
            .await?
        else {
            // A position created and burned in one block can no longer be read.
            return Ok(None);
        };
        let factory: Address = self
            .factory
            .parse()
            .map_err(|_| ExampleError::InvalidChainConfig)?;
        let pool = self
            .call(
                factory,
                event.block_hash,
                FactoryMetadata::getPoolCall {
                    tokenA: result.token0,
                    tokenB: result.token1,
                    fee: result.fee,
                },
            )
            .await?
            .ok_or(ExampleError::InvalidPoolState)?
            .to_string()
            .to_lowercase();
        self.transaction(store, context).await?;
        Ok(Some(Position {
            id,
            owner: Address::ZERO.to_string().to_lowercase(),
            pool: pool.clone(),
            token0: result.token0.to_string().to_lowercase(),
            token1: result.token1.to_string().to_lowercase(),
            tick_lower: format!("{pool}#{}", result.tickLower),
            tick_upper: format!("{pool}#{}", result.tickUpper),
            transaction: context.tx.clone(),
            fee_growth_inside0_last_x128: integer(&result.feeGrowthInside0LastX128.to_string()),
            fee_growth_inside1_last_x128: integer(&result.feeGrowthInside1LastX128.to_string()),
            ..Default::default()
        }))
    }

    /// Refreshes a position's fee-growth fields from the manager contract.
    async fn position_fees<T>(
        &self,
        position: &mut Position,
        event: &Parsed<T>,
        token_id: U256,
    ) -> RavenResult<()> {
        if let Some(result) = self
            .call(
                self.position_manager,
                event.block_hash,
                PositionManager::positionsCall { tokenId: token_id },
            )
            .await?
        {
            position.fee_growth_inside0_last_x128 =
                integer(&result.feeGrowthInside0LastX128.to_string());
            position.fee_growth_inside1_last_x128 =
                integer(&result.feeGrowthInside1LastX128.to_string());
        }
        Ok(())
    }

    /// Writes the position's once-per-block snapshot.
    async fn snapshot_position(
        &self,
        store: &mut dyn EntityStore,
        position: &Position,
        context: &Context,
    ) -> RavenResult<()> {
        let id = format!("{}#{}", position.id, context.number);
        self.transaction(store, context).await?;
        let snapshot = PositionSnapshot {
            id: id.clone(),
            owner: position.owner.clone(),
            pool: position.pool.clone(),
            position: position.id.clone(),
            block_number: integer(&context.number.to_string()),
            timestamp: integer(&context.timestamp.to_string()),
            liquidity: position.liquidity.clone(),
            deposited_token0: position.deposited_token0.clone(),
            deposited_token1: position.deposited_token1.clone(),
            withdrawn_token0: position.withdrawn_token0.clone(),
            withdrawn_token1: position.withdrawn_token1.clone(),
            collected_fees_token0: position.collected_fees_token0.clone(),
            collected_fees_token1: position.collected_fees_token1.clone(),
            transaction: context.tx.clone(),
            fee_growth_inside0_last_x128: position.fee_growth_inside0_last_x128.clone(),
            fee_growth_inside1_last_x128: position.fee_growth_inside1_last_x128.clone(),
        };
        store.save(&snapshot).await
    }

    /// Applies liquidity changes and deposited or withdrawn token amounts.
    async fn position_liquidity<T>(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<T>,
        token_id: U256,
        liquidity: u128,
        raw0: U256,
        raw1: U256,
        increase: bool,
    ) -> RavenResult<()> {
        if event.block_number == 14_317_993 {
            return Ok(());
        }
        let context = self.context(event).await?;
        let Some(mut position) = self.position(store, event, token_id, &context).await? else {
            return Ok(());
        };
        if position.pool == "0x8fe8d9bb8eeba3ed688069c3d6b556c9ca258248" {
            return Ok(());
        }
        let (Some(token0), Some(token1)) = (
            store.load::<Token>(&position.token0).await?,
            store.load::<Token>(&position.token1).await?,
        ) else {
            return Ok(());
        };
        let liquidity = integer(&liquidity.to_string());
        position.liquidity = if increase {
            position.liquidity.clone() + liquidity
        } else {
            position.liquidity.clone() - liquidity
        };
        let amount0 = units(&raw0.to_string(), &token0.decimals.to_string());
        let amount1 = units(&raw1.to_string(), &token1.decimals.to_string());
        if increase {
            position.deposited_token0 = round34(position.deposited_token0.clone() + amount0);
            position.deposited_token1 = round34(position.deposited_token1.clone() + amount1);
        } else {
            position.withdrawn_token0 = round34(position.withdrawn_token0.clone() + amount0);
            position.withdrawn_token1 = round34(position.withdrawn_token1.clone() + amount1);
        }
        self.position_fees(&mut position, event, token_id).await?;
        store.save(&position).await?;
        self.snapshot_position(store, &position, &context).await
    }

    /// Adds collected fees to an existing indexed position.
    async fn position_collect(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<PositionManager::Collect>,
    ) -> RavenResult<()> {
        let context = self.context(event).await?;
        let Some(mut position) = self
            .position(store, event, event.value.tokenId, &context)
            .await?
        else {
            return Ok(());
        };
        if position.pool == "0x8fe8d9bb8eeba3ed688069c3d6b556c9ca258248" {
            return Ok(());
        }
        if let Some(token0) = store.load::<Token>(&position.token0).await? {
            let amount = units(
                &event.value.amount0.to_string(),
                &token0.decimals.to_string(),
            );
            // The indexed rule adds amount0/decimals0 to both fee fields.
            position.collected_fees_token0 =
                round34(position.collected_fees_token0.clone() + amount.clone());
            position.collected_fees_token1 =
                round34(position.collected_fees_token1.clone() + amount);
        }
        self.position_fees(&mut position, event, event.value.tokenId)
            .await?;
        store.save(&position).await?;
        self.snapshot_position(store, &position, &context).await
    }

    /// Updates ownership for a tracked position-manager transfer.
    async fn position_transfer(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<PositionManager::Transfer>,
    ) -> RavenResult<()> {
        let context = self.context(event).await?;
        let Some(mut position) = self
            .position(store, event, event.value.tokenId, &context)
            .await?
        else {
            return Ok(());
        };
        position.owner = event.value.to.to_string().to_lowercase();
        store.save(&position).await?;
        self.snapshot_position(store, &position, &context).await
    }

    /// Dispatches decoded position-manager events to their position handlers.
    pub(crate) async fn position_event(
        &self,
        store: &mut dyn EntityStore,
        log: &LogUpdate,
    ) -> RavenResult<()> {
        let signature = log.log.topics().first();
        macro_rules! decode {
            ($event:ty) => {
                log.parsed(
                    <$event>::decode_log_validate(&log.log)
                        .map_err(|e| RavenError::Parser(Box::new(e)))?
                        .data,
                )
            };
        }
        if signature == Some(&PositionManager::IncreaseLiquidity::SIGNATURE_HASH) {
            let event = decode!(PositionManager::IncreaseLiquidity);
            let value = &event.value;
            self.position_liquidity(
                store,
                &event,
                value.tokenId,
                value.liquidity,
                value.amount0,
                value.amount1,
                true,
            )
            .await
        } else if signature == Some(&PositionManager::DecreaseLiquidity::SIGNATURE_HASH) {
            let event = decode!(PositionManager::DecreaseLiquidity);
            let value = &event.value;
            self.position_liquidity(
                store,
                &event,
                value.tokenId,
                value.liquidity,
                value.amount0,
                value.amount1,
                false,
            )
            .await
        } else if signature == Some(&PositionManager::Collect::SIGNATURE_HASH) {
            self.position_collect(store, &decode!(PositionManager::Collect))
                .await
        } else if signature == Some(&PositionManager::Transfer::SIGNATURE_HASH) {
            self.position_transfer(store, &decode!(PositionManager::Transfer))
                .await
        } else {
            Ok(())
        }
    }
}

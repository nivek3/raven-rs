//! Fee-growth reads and bounded crossed-tick updates.
use alloy_provider::Provider;
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use raven_engine::{EntityStore, EntityStoreExt, RavenResult};
use raven_evm::Parsed;

use crate::{
    ExampleError,
    entities::{Pool, Tick},
    events::{Flash, PoolMetadata},
    handlers::{Context, Mapping},
    math::integer,
};

impl<P: Provider> Mapping<P> {
    /// Reads global pool fee-growth values at the event block.
    pub(crate) async fn pool_fee_vars<T>(
        &self,
        pool: &mut Pool,
        event: &Parsed<T>,
    ) -> RavenResult<()> {
        let address = event.address.ok_or(ExampleError::MissingMetadata)?;
        let fee0 = self
            .call(
                address,
                event.block_hash,
                PoolMetadata::feeGrowthGlobal0X128Call,
            )
            .await?
            .ok_or(ExampleError::InvalidPoolState)?;
        let fee1 = self
            .call(
                address,
                event.block_hash,
                PoolMetadata::feeGrowthGlobal1X128Call,
            )
            .await?
            .ok_or(ExampleError::InvalidPoolState)?;
        pool.fee_growth_global0_x128 = integer(&fee0.to_string());
        pool.fee_growth_global1_x128 = integer(&fee1.to_string());
        Ok(())
    }

    /// Reads initialized tick fee-growth values at the event block.
    pub(crate) async fn tick_fee_vars<T>(
        &self,
        store: &mut dyn EntityStore,
        tick: &mut Tick,
        event: &Parsed<T>,
        context: &Context,
    ) -> RavenResult<()> {
        let index = tick
            .tick_idx
            .to_i32()
            .ok_or(ExampleError::InvalidPoolState)?;
        let result = self
            .call(
                event.address.ok_or(ExampleError::MissingMetadata)?,
                event.block_hash,
                PoolMetadata::ticksCall {
                    tick: index
                        .try_into()
                        .map_err(|_| ExampleError::InvalidPoolState)?,
                },
            )
            .await?
            .ok_or(ExampleError::InvalidPoolState)?;
        tick.fee_growth_outside0_x128 = integer(&result.feeGrowthOutside0X128.to_string());
        tick.fee_growth_outside1_x128 = integer(&result.feeGrowthOutside1X128.to_string());
        store.save(tick).await?;
        crate::intervals::tick_day(store, context, tick).await
    }

    /// Refreshes a tick's fee-growth fields and its daily aggregate.
    async fn update_tick<T>(
        &self,
        store: &mut dyn EntityStore,
        pool: &str,
        index: &BigDecimal,
        event: &Parsed<T>,
        context: &Context,
    ) -> RavenResult<()> {
        let id = format!("{pool}#{index}");
        if let Some(mut tick) = store.load::<Tick>(&id).await? {
            self.tick_fee_vars(store, &mut tick, event, context).await?;
        }
        Ok(())
    }

    /// Refreshes the current initialized tick crossed by a swap.
    pub(crate) async fn crossed_ticks<T>(
        &self,
        store: &mut dyn EntityStore,
        pool: &Pool,
        old: Option<BigDecimal>,
        event: &Parsed<T>,
        context: &Context,
    ) -> RavenResult<()> {
        let (Some(old), Some(new)) = (old, pool.tick.clone()) else {
            return Ok(());
        };
        let spacing = match pool.fee_tier.to_i32() {
            Some(10_000) => integer("200"),
            Some(3_000) => integer("60"),
            Some(500) => integer("10"),
            Some(100) => integer("1"),
            _ => return Err(ExampleError::InvalidPoolState.into()),
        };
        let modulo = &new % &spacing;
        if modulo == integer("0") {
            self.update_tick(store, &pool.id, &new, event, context)
                .await?;
        }
        let distance = if old > new { &old - &new } else { &new - &old };
        // Skip tick refresh when the integer spacing distance exceeds 100.
        if distance >= &spacing * integer("101") {
            return Ok(());
        }
        // Use the new tick's signed remainder to choose crossed spacing boundaries.
        if new > old {
            let mut index = &old + (&spacing - &modulo);
            while index <= new {
                self.update_tick(store, &pool.id, &index, event, context)
                    .await?;
                index += spacing.clone();
            }
        } else if new < old {
            let mut index = &old - &modulo;
            while index >= new {
                self.update_tick(store, &pool.id, &index, event, context)
                    .await?;
                index -= spacing.clone();
            }
        }
        Ok(())
    }

    /// Applies Flash fee growth to the indexed pool.
    pub(crate) async fn flashed(
        &self,
        store: &mut dyn EntityStore,
        event: &Parsed<Flash>,
    ) -> RavenResult<()> {
        let id = event
            .address
            .ok_or(ExampleError::MissingMetadata)?
            .to_string()
            .to_lowercase();
        let Some(mut pool) = store.load::<Pool>(&id).await? else {
            return Ok(());
        };
        self.pool_fee_vars(&mut pool, event).await?;
        // Flash updates fee growth without creating an entity or changing event counts.
        store.save(&pool).await
    }
}
